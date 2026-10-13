use super::*;
use crate::canvas::Edit;
use reshiki::document::{Document, Point};

fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.doc.add_atom("C", Point::default());
    let source = app.tab.doc.add_atom("C", Point::new(180., 0.));
    let end = app.tab.doc.add_atom("C", Point::new(222., 0.));
    app.tab.doc.add_bond(source, end, 1, "plain");
    app.tab.selected = vec![source];
    app
}
#[test]
fn joining_uses_the_preview_and_is_one_undo_step() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Join(Action::Begin));
    assert_eq!(app.tab.doc, before);
    let state = app.tab.joining.as_ref().unwrap();
    let (expected, _) = state
        .prepared
        .place(
            Point::default(),
            None,
            10. / app.tab.camera.zoom,
            state.anchor,
            state.mode,
        )
        .unwrap();
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, expected);
    assert_eq!(app.tab.doc.bonds.len(), 2);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, expected);
}
#[test]
fn cancel_switching_tools_and_invalid_targets_keep_the_original() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Canvas(Edit::Template(
        Point::new(500., 500.),
        None,
    )));
    assert!(app.tab.joining.is_some());
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Escape);
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Tool(Tool::Bond(2)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tool, Tool::Bond(2));
    assert_eq!(app.tab.doc, before);
}
#[test]
fn a_changed_document_cannot_be_overwritten_by_a_prepared_join() {
    let mut app = ready();
    let _ = app.update(Message::Join(Action::Begin));
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("O", Point::new(300., 100.));
    app.changed(before);
    let changed = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, changed);
    assert!(app.error);
}
#[test]
fn anchor_picker_switches_between_atom_and_bond_modes() {
    let mut app = ready();
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Bond(2, 3))));
    assert_eq!(app.tab.joining.as_ref().unwrap().mode, Connection::FuseBond);
    let _ = app.update(Message::Join(Action::Mode(Connection::ShareAtom)));
    assert!(matches!(
        app.tab.joining.as_ref().unwrap().anchor,
        Anchor::Atom(_)
    ));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(3))));
    assert_eq!(app.tab.joining.as_ref().unwrap().anchor, Anchor::Atom(3));
}

#[test]
fn generic_join_rejections_keep_document_history_and_show_expand_instruction() {
    use reshiki::atom_text::{self, Mode};
    for generic_id in [3, 1] {
        let mut app = ready();
        app.tab.doc = atom_text::apply(&app.tab.doc, generic_id, "[C,N]", Mode::Auto).unwrap();
        // Keep both real undo and redo frames so a rejected join cannot quietly
        // clear either side of history. These unrelated ordinary edits remain valid.
        for y in [80., 160.] {
            let before = app.tab.doc.clone();
            app.tab.doc.add_atom("O", Point::new(400., y));
            app.changed(before);
        }
        let _ = app.update(Message::Undo);
        app.tab.selected = vec![2];
        assert!(app.tab.history.can_undo() && app.tab.history.can_redo());
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        let history = (app.tab.history.can_undo(), app.tab.history.can_redo());
        let _ = app.update(Message::Join(Action::Begin));
        if generic_id == 1 {
            assert!(app.tab.joining.is_some());
            let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
            assert!(app.tab.joining.is_some());
        } else {
            assert!(app.tab.joining.is_none());
        }
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(
            (app.tab.history.can_undo(), app.tab.history.can_redo()),
            history
        );
        assert_eq!(app.status, reshiki::generic::JOIN_NOTICE);
        assert!(app.error);
    }
}

fn coordination() -> (App, u64, u64, u64) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let co = app.tab.doc.add_atom("Co", Point::default());
    app.tab.doc.atom_mut(co).unwrap().charge = 3;
    let n = app.tab.doc.add_atom("N", Point::new(40., -25.));
    let c = app.tab.doc.add_atom("C", Point::new(70., -15.));
    let c2 = app.tab.doc.add_atom("C", Point::new(70., 15.));
    let n2 = app.tab.doc.add_atom("N", Point::new(40., 25.));
    for (a, b) in [(n, c), (c, c2), (c2, n2)] {
        app.tab.doc.add_bond(a, b, 1, "plain");
    }
    for id in [n, n2] {
        app.tab.doc.atom_mut(id).unwrap().label_h = 2;
    }
    app.tab.selected = vec![n];
    (app, co, n, n2)
}

#[test]
fn first_and_second_chelate_contacts_are_in_place_cancellable_and_one_undo_each() {
    let (mut app, co, n, n2) = coordination();
    let original = app.tab.doc.clone();
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Join(Action::Mode(Connection::Coordinate)));
    assert_eq!(
        app.tab.joining.as_ref().unwrap().mode,
        Connection::Coordinate
    );
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(n))));
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert!(
        app.tab
            .doc
            .bonds
            .iter()
            .any(|b| (b.a, b.b, b.order) == (n, co, 5))
    );
    let first = app.tab.doc.clone();
    app.tab.selected = vec![n2];
    let _ = app.update(Message::Join(Action::BeginCoordination));
    assert!(app.tab.joining.is_some());
    let _ = app.update(Message::Escape);
    assert_eq!(app.tab.doc, first);
    let _ = app.update(Message::Join(Action::BeginCoordination));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(n2))));
    let _ = app.update(Message::Canvas(Edit::Template(
        Point::default(),
        Some(Point::new(300., 200.)),
    )));
    let second = app.tab.doc.clone();
    assert_eq!(second.atoms, original.atoms);
    assert_eq!(second.bonds.len(), 5);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, first);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, second);
    app.tab.selected = vec![n];
    let _ = app.update(Message::Join(Action::BeginCoordination));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(n))));
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert_eq!(app.tab.doc, second);
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.tab.doc, first,
        "Duplicate contact must not add Undo history"
    );
}

#[test]
fn coordination_projection_flip_changes_tip_and_never_chemical_direction() {
    let (mut app, co, n, _) = coordination();
    app.tab.doc = reshiki::joining::coordinate_atoms(&app.tab.doc, n, co)
        .unwrap()
        .into_document(&app.tab.doc);
    app.tab.selected = vec![n, co];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::ApplyBondPreset(reshiki::bonds::BondPreset::Wedge));
    let donor_narrowed = app.tab.doc.clone();
    let _ = app.update(Message::ReverseBonds);
    let reversed = app.tab.doc.clone();
    let bond = reversed.bonds.iter().find(|b| b.order == 5).unwrap();
    assert_eq!(
        (bond.a, bond.b, bond.order, bond.display.as_str()),
        (n, co, 5, "wedge_end")
    );
    assert_eq!(reversed.atoms, before.atoms);
    assert_eq!(
        reshiki::bonds::BondPreset::of(bond),
        Some(reshiki::bonds::BondPreset::Wedge)
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, donor_narrowed);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, reversed);
    let _ = app.update(Message::ApplyBondPreset(
        reshiki::bonds::BondPreset::HashedWedge,
    ));
    let bond = app.tab.doc.bonds.iter().find(|b| b.order == 5).unwrap();
    assert_eq!((bond.a, bond.b, bond.display.as_str()), (n, co, "hash_end"));
}

#[tokio::test]
async fn coordination_export_reports_the_measured_drawing_limits() -> Result<(), String> {
    let (app, co, n, _) = coordination();
    let mut doc =
        reshiki::joining::coordinate_atoms(&app.tab.doc, n, co)?.into_document(&app.tab.doc);
    let bond = doc.bonds.iter_mut().find(|b| b.order == 5).unwrap();
    reshiki::bonds::BondPreset::Wedge.apply(bond);
    assert!(bond.reverse_projection());
    for format in ["cdxml", "cdx", "mol"] {
        let mut request = reshiki::engine::Request::molecule("export", doc.clone());
        request.format = Some(format.into());
        let response = app.engine.request(request).await?;
        assert!(response.output.is_some());
        let expected = if format == "mol" {
            "omits solid/hashed coordination projection appearance"
        } else {
            "ChemDraw 26 displays dative arrows"
        };
        assert!(
            response.warnings.iter().any(|w| w.contains(expected)),
            "Missing {format} limitation: {:?}",
            response.warnings
        );
    }
    Ok(())
}

#[test]
fn direct_join_commands_share_selection_requirements_and_routes() {
    use super::super::shortcuts;
    let mut app = ready();
    for (selection, join, merge) in [
        (vec![], false, false),
        (vec![1], false, false),
        (vec![1, 2], true, false),
        (vec![3, 2, 1], true, true),
        (vec![1, 2, 999], false, false),
    ] {
        app.tab.selected = selection;
        let commands = app.selected_join_commands();
        assert_eq!(commands[0].enabled, join);
        assert_eq!(commands[1].enabled, merge);
        assert!(matches!(
            commands[0].message,
            Message::Shortcut(shortcuts::Action::Join)
        ));
        assert!(matches!(
            commands[1].message,
            Message::Shortcut(shortcuts::Action::MergeAtoms)
        ));
    }
    app.tab.doc.atom_mut(2).unwrap().centroid = vec![1, 3];
    app.tab.selected = vec![1, 2, 3];
    assert!(app.selected_join_commands().iter().all(|c| !c.enabled));
}

#[test]
fn both_atom_merge_commands_use_rendered_ink_and_exclude_unselected_objects() {
    use super::super::shortcuts;
    use reshiki::{document::Annotation, palette::Color, scene::Primitive, typography::TextStyle};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/join-three-atoms-before.rsk"
    ))
    .unwrap();
    app.tab.doc.atom_labels.hydrogens = false;
    app.tab.doc.atom_labels.carbons = reshiki::atom_labels::Carbons::All;
    let ink = [17, 71, 149];
    for (id, size_pt) in [(1, 32.), (3, 10.), (5, 24.)] {
        app.tab.doc.atom_mut(id).unwrap().text_style = Some(TextStyle {
            size_pt,
            color: Color::Custom(ink),
            ..Default::default()
        });
    }
    app.tab.doc.annotations.push(Annotation {
        id: 7,
        position: Point::new(-1000., -1500.),
        text: "Unselected caption".into(),
        format: Default::default(),
    });
    app.tab.doc.atom_mut(1).unwrap().display.number = Some(reshiki::atom_labels::Number {
        text: "999".into(),
        offset: Some(Point::new(900., 900.)),
        style: Default::default(),
    });
    let before = app.tab.doc.clone();
    // Measure the actual rendered text primitives independently of the app
    // helper. Only the three selected labels have this ink color.
    let boxes: Vec<_> = reshiki::scene::primitives(&before)
        .iter()
        .flat_map(|primitive| match primitive {
            Primitive::Text {
                position,
                text,
                size,
                color,
                style,
            } if *color == ink => reshiki::style::text_ink_boxes(text, *size, style)
                .into_iter()
                .map(|(lo, hi)| (position.offset(lo.x, lo.y), position.offset(hi.x, hi.y)))
                .collect(),
            _ => vec![],
        })
        .collect();
    assert_eq!(boxes.len(), 3);
    let lo = Point::new(
        boxes.iter().map(|(lo, _)| lo.x).reduce(f32::min).unwrap(),
        boxes.iter().map(|(lo, _)| lo.y).reduce(f32::min).unwrap(),
    );
    let hi = Point::new(
        boxes.iter().map(|(_, hi)| hi.x).reduce(f32::max).unwrap(),
        boxes.iter().map(|(_, hi)| hi.y).reduce(f32::max).unwrap(),
    );
    let expected = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    assert!(expected.distance(Point::default()) > 0.1);
    assert!(expected.distance(Point::new(0., 2.)) > 0.1);
    for action in [shortcuts::Action::Join, shortcuts::Action::MergeAtoms] {
        app.tab.doc = before.clone();
        app.tab.selected = vec![1, 3, 5];
        let _ = app.update(Message::Shortcut(action));
        assert!(!app.error, "{}", app.status);
        assert!(app.tab.doc.atom(1).unwrap().position.distance(expected) < 0.0001);
        assert_eq!(app.tab.selected, [1]);
        assert_eq!(
            app.tab.doc.atom(1).unwrap().text_style,
            before.atom(1).unwrap().text_style
        );
        assert_eq!(app.tab.doc.annotations, before.annotations);
        for id in [2, 4, 6] {
            assert_eq!(
                app.tab.doc.atom(id).unwrap().position,
                before.atom(id).unwrap().position
            );
        }
    }
}

#[test]
fn hidden_selected_labels_use_atom_bounds_without_terminal_arms() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.selected.clear();
    for (position, end) in [
        (Point::new(0., 0.), Point::new(-500., -500.)),
        (Point::new(20., 0.), Point::new(800., -500.)),
        (Point::new(0., 60.), Point::new(0., 1000.)),
    ] {
        let id = app.tab.doc.add_atom("C", position);
        let terminal = app.tab.doc.add_atom("C", end);
        app.tab.doc.add_bond(id, terminal, 1, "plain");
        app.tab.selected.push(id);
        assert!(
            reshiki::scene::atom_label_ink_bounds(app.tab.doc.atom(id).unwrap(), &app.tab.doc)
                .is_none()
        );
    }
    let (joined, _) = app.merge_selected_atoms().unwrap();
    assert_eq!(joined.atom(1).unwrap().position, Point::new(10., 30.));
}

#[test]
fn native_fixture_visual_center_uses_recomputed_labels() {
    use reshiki::atom_labels::refresh::Refresh;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/join-three-atoms-before.rsk"
    ))
    .unwrap();
    reshiki::atom_labels::clear_computed(&mut app.tab.doc);
    let refreshed = Refresh::calculate(&app.tab.doc, &Refresh::default()).unwrap();
    assert!(refreshed.notice.is_none(), "{:?}", refreshed.notice);
    refreshed.apply(&mut app.tab.doc);
    app.tab.selected = vec![1, 3, 5];
    assert_eq!(app.tab.doc.atom(1).unwrap().label_h, 2);
    assert_eq!(app.tab.doc.atom(5).unwrap().label_h, 1);
    let (geometry, extents): (Vec<_>, Vec<_>) = app
        .tab
        .selected
        .iter()
        .map(|id| {
            let atom = app.tab.doc.atom(*id).unwrap();
            let ink = reshiki::scene::atom_label_ink_bounds(atom, &app.tab.doc);
            let effective = ink.unwrap_or((atom.position, atom.position));
            (
                serde_json::json!({
                    "id": id, "element": atom.element, "position": atom.position,
                    "label_h": atom.label_h, "label_ink_bounds": ink,
                    "effective_bounds": effective,
                    "point_fallback": ink.is_none(),
                }),
                effective,
            )
        })
        .unzip();
    let (lo, hi) = extents
        .into_iter()
        .reduce(|(a, b), (lo, hi)| {
            (
                Point::new(a.x.min(lo.x), a.y.min(lo.y)),
                Point::new(b.x.max(hi.x), b.y.max(hi.y)),
            )
        })
        .unwrap();
    let union_midpoint = Point::new(
        ((f64::from(lo.x) + f64::from(hi.x)) / 2.) as f32,
        ((f64::from(lo.y) + f64::from(hi.y)) / 2.) as f32,
    );
    let (joined, ids) = app.merge_selected_atoms().unwrap();
    let center = joined.atom(1).unwrap().position;
    assert_eq!(ids, [1]);
    assert_eq!(center, union_midpoint);
    assert_eq!(
        app.join_shortcut().unwrap().0.atom(1).unwrap().position,
        center
    );
    assert_ne!(center, Point::default());
    println!(
        "JOIN_NATIVE_FIXTURE_EXPECTED_GEOMETRY={}",
        serde_json::json!({
            "fixture": "tests/fixtures/join-three-atoms-before.rsk",
            "label_state": "normal native load: clear computed, Refresh::calculate, Refresh::apply",
            "selected_ids": app.tab.selected, "selected_geometry": geometry,
            "selected_visual_union_bounds": (lo, hi),
            "selected_visual_union_midpoint": union_midpoint,
            "selected_node_arithmetic_mean": Point::default(),
            "selected_node_geometric_bounds_center": Point::new(0., 2.),
            "app_visual_bounds_center": center, "expected_survivor_id": 1,
            "expected_survivor_position": center, "expected_atom_count": 4,
            "expected_bond_count": 3,
        })
    );
}

#[test]
fn join_routes_statuses_and_generic_admission_match_the_baseline_table() {
    use super::super::shortcuts;
    use reshiki::atom_text::{self, Mode};

    struct Case {
        name: &'static str,
        elements: &'static [&'static str],
        bonds: &'static [(u64, u64)],
        selected: &'static [u64],
        route: JoinPlan,
        merge: bool,
        generic_sites: &'static [u64],
    }
    let cases = [
        Case {
            name: "share atom",
            elements: &["C", "C"],
            bonds: &[],
            selected: &[1, 2],
            route: JoinPlan::ShareAtom {
                source: 1,
                target: 2,
            },
            merge: false,
            generic_sites: &[1, 2],
        },
        Case {
            name: "coordinate donor first",
            elements: &["N", "Co"],
            bonds: &[],
            selected: &[1, 2],
            route: JoinPlan::Coordinate { donor: 1, metal: 2 },
            merge: false,
            generic_sites: &[1],
        },
        Case {
            name: "coordinate metal first",
            elements: &["N", "Co"],
            bonds: &[],
            selected: &[2, 1],
            route: JoinPlan::Coordinate { donor: 1, metal: 2 },
            merge: false,
            generic_sites: &[1],
        },
        Case {
            name: "fuse bonds in document order",
            elements: &["C", "C", "C", "C"],
            bonds: &[(3, 4), (1, 2)],
            selected: &[4, 2, 3, 1],
            route: JoinPlan::FuseBond {
                source: (3, 4),
                target: (1, 2),
            },
            merge: false,
            generic_sites: &[3, 1],
        },
        Case {
            name: "merge four atoms with shared bond endpoint",
            elements: &["C", "C", "C", "C"],
            bonds: &[(1, 2), (2, 3)],
            selected: &[1, 2, 3, 4],
            route: JoinPlan::Merge,
            merge: false,
            generic_sites: &[2],
        },
        Case {
            name: "merge four atoms with three bonds",
            elements: &["C", "C", "C", "C"],
            bonds: &[(1, 2), (2, 3), (3, 4)],
            selected: &[1, 2, 3, 4],
            route: JoinPlan::Merge,
            merge: false,
            generic_sites: &[3],
        },
        Case {
            name: "merge three atoms through Join",
            elements: &["C", "C", "C"],
            bonds: &[],
            selected: &[3, 1, 2],
            route: JoinPlan::Merge,
            merge: false,
            generic_sites: &[1],
        },
        Case {
            name: "merge three atoms explicitly",
            elements: &["C", "C", "C"],
            bonds: &[],
            selected: &[3, 1, 2],
            route: JoinPlan::Merge,
            merge: true,
            generic_sites: &[1],
        },
    ];
    for case in cases {
        for generic_site in
            std::iter::once(None).chain(case.generic_sites.iter().copied().map(Some))
        {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tab.doc = Document::default();
            for (element, x) in case.elements.iter().zip([0., 42., 180., 222.]) {
                app.tab.doc.add_atom(element, Point::new(x, 0.));
            }
            for &(a, b) in case.bonds {
                app.tab.doc.add_bond(a, b, 1, "plain");
            }
            if let Some(id) = generic_site {
                app.tab.doc = atom_text::apply(&app.tab.doc, id, "[C,N]", Mode::Auto).unwrap();
            }
            app.tab.selected = case.selected.to_vec();
            let message = Message::Shortcut(if case.merge {
                shortcuts::Action::MergeAtoms
            } else {
                shortcuts::Action::Join
            });
            let blocked = generic_site.is_some();
            assert_eq!(
                app.generic_join_rejection(&message),
                blocked,
                "{} {generic_site:?}",
                case.name
            );
            let plan = if case.merge {
                Some(JoinPlan::Merge)
            } else {
                reshiki::joining::join_plan(&app.tab.doc, &app.tab.selected)
            };
            assert_eq!(plan, Some(case.route), "{} {generic_site:?}", case.name);
            assert_eq!(
                case.route.generic_blocked(&app.tab.doc, &app.tab.selected),
                blocked,
                "{} {generic_site:?}",
                case.name
            );
            let before = app.tab.doc.clone();
            let expected = if blocked {
                (before.clone(), app.tab.selected.clone())
            } else {
                match case.route {
                    JoinPlan::Coordinate { donor, metal } => (
                        reshiki::joining::coordinate_atoms(&before, donor, metal)
                            .unwrap()
                            .into_document(&before),
                        vec![donor, metal],
                    ),
                    JoinPlan::ShareAtom { source, target } => Prepared::new(&before, &[source])
                        .unwrap()
                        .place(
                            before.atom(target).unwrap().position,
                            None,
                            1.,
                            Anchor::Atom(source),
                            Connection::ShareAtom,
                        )
                        .unwrap(),
                    JoinPlan::FuseBond { source, target } => {
                        let a = before.atom(target.0).unwrap().position;
                        let b = before.atom(target.1).unwrap().position;
                        Prepared::new(&before, &[source.0, source.1])
                            .unwrap()
                            .place(
                                Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.),
                                None,
                                1.,
                                Anchor::Bond(source.0, source.1),
                                Connection::FuseBond,
                            )
                            .unwrap()
                    }
                    JoinPlan::Merge => app.merge_selected_atoms().unwrap(),
                }
            };
            let history = app.tab.history.undo_frames().len();
            let revision = app.tab.revision;
            let _ = app.update(message);
            assert_eq!(
                app.error, blocked,
                "{} {generic_site:?}: {}",
                case.name, app.status
            );
            assert_eq!(
                app.status,
                if blocked {
                    reshiki::generic::JOIN_NOTICE
                } else if case.merge {
                    "Merged selected atoms"
                } else {
                    "Joined selected atoms or bonds"
                },
                "{} {generic_site:?}",
                case.name
            );
            assert_eq!(app.tab.doc, expected.0, "{} {generic_site:?}", case.name);
            assert_eq!(
                app.tab.selected, expected.1,
                "{} {generic_site:?}",
                case.name
            );
            assert_eq!(
                app.tab.doc != before,
                !blocked,
                "{} {generic_site:?}",
                case.name
            );
            assert_eq!(
                app.tab.history.undo_frames().len(),
                history + usize::from(!blocked),
                "{} {generic_site:?}",
                case.name
            );
            assert_eq!(
                app.tab.revision,
                revision + u64::from(!blocked),
                "{} {generic_site:?}",
                case.name
            );
            assert!(!app.tab.history.can_redo());
        }
    }
}

#[test]
fn repeated_canvas_coordination_reports_duplicate_without_another_history_frame() {
    let (mut app, co, donor, _) = coordination();
    let original = app.tab.doc.clone();
    let point = app.tab.doc.atom(co).unwrap().position;
    let history = app.tab.history.undo_frames().len();
    for (status, growth) in [
        ("Donor → metal contact added in place · Undo removes it", 1),
        ("This donor is already coordinated to that metal", 1),
    ] {
        let before = app.tab.doc.clone();
        app.tab.selected = vec![donor];
        let _ = app.update(Message::Join(Action::BeginCoordination));
        let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(donor))));
        let _ = app.update(Message::Canvas(Edit::Template(point, None)));
        assert_eq!(app.status, status);
        assert!(!app.error);
        assert!(app.tab.joining.is_none());
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(app.tab.doc.atoms, original.atoms);
        assert_eq!(app.tab.doc.bonds.len(), original.bonds.len() + 1);
        assert_eq!(app.tab.history.undo_frames().len(), history + growth);
        if before != original {
            assert_eq!(app.tab.doc, before);
        }
    }
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}

#[tokio::test]
async fn duplicate_canvas_coordination_after_a_label_refresh_reports_the_installed_copy_as_an_edit()
{
    use iced::futures::StreamExt;

    let (mut app, co, donor, _) = coordination();
    let point = app.tab.doc.atom(co).unwrap().position;
    let reshiki::joining::Coordination::Added(contact) =
        reshiki::joining::coordinate_atoms(&app.tab.doc, donor, co).unwrap()
    else {
        panic!("The first contact is added");
    };
    app.tab.doc = contact;
    app.tab.selected = vec![donor];
    let _ = app.update(Message::Join(Action::BeginCoordination));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(donor))));
    // A label calculation of this revision lands while the preparation is open.
    app.tab.labels_dirty = true;
    let mut task = iced_runtime::task::into_stream(app.start_label_refresh())
        .expect("A real label calculation starts");
    let Some(iced_runtime::Action::Output(labels)) = task.next().await else {
        panic!("The label task must return its tagged completion");
    };
    assert!(matches!(&labels, Message::LabelsReady(_, Ok(_))));
    let revision = app.tab.revision;
    let _ = app.update(labels);
    assert_eq!(app.tab.revision, revision);
    assert_ne!(
        app.tab.doc,
        app.tab.joining.as_ref().unwrap().prepared.original,
        "The refresh updates the drawing in place"
    );
    let history = app.tab.history.undo_frames().len();
    let _ = app.update(Message::Canvas(Edit::Template(point, None)));
    assert_eq!(
        app.status,
        "Donor → metal contact added in place · Undo removes it"
    );
    assert!(!app.error);
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.history.undo_frames().len(), history + 1);
}

#[test]
fn coordination_status_pins_added_and_duplicate_with_and_without_a_mark() {
    for (outcome, marked, status) in [
        (
            Coordination::Added(Document::default()),
            false,
            "Donor → metal contact added in place · Undo removes it",
        ),
        (
            Coordination::AlreadyPresent,
            false,
            "This donor is already coordinated to that metal",
        ),
        (
            Coordination::Added(Document::default()),
            true,
            "Marked donor → metal contact added · Undo removes it",
        ),
        (
            Coordination::AlreadyPresent,
            true,
            "This marked donor is already coordinated to that metal",
        ),
    ] {
        assert_eq!(coordination_status(&outcome, marked), status);
    }
}

#[test]
fn coordination_panel_names_the_donor_and_uses_one_short_hint() {
    let (mut app, _, n, _) = coordination();
    let _ = app.update(Message::Join(Action::BeginCoordination));
    let state = app.tab.joining.as_ref().unwrap();
    assert_eq!(anchor_label(state), format!("Donor: N (atom {n})"));
    let texts = join_mode_texts(Connection::Coordinate);
    assert_eq!(texts.title, "Coordinate in place");
    assert_eq!(
        texts.intro,
        "Pick the donor (N, O, S or P) in the preview, then click a metal on the drawing."
    );
    assert_eq!(texts.instruction, None, "The intro says what a click does");
    assert_eq!(
        texts.footer,
        "Atoms stay in place · Escape cancels · One Undo step per contact"
    );
    assert_eq!(texts.bar_subtitle, "Pick a donor, then a metal");
    // The mode pick list switches back to Move & attach and its wording.
    let _ = app.update(Message::Join(Action::Mode(Connection::Connect)));
    let state = app.tab.joining.as_ref().unwrap();
    assert_eq!(anchor_label(state), format!("Atom {n}"));
    let texts = join_mode_texts(state.mode);
    assert_eq!(texts.title, "Move & attach");
    assert_eq!(texts.instruction, Some(CONNECT_DETAIL));
}

#[test]
fn row_and_menu_share_the_attach_and_coordinate_commands() {
    let mut app = ready();
    let row: Vec<_> = app
        .selected_attach_commands()
        .into_iter()
        .map(|c| (c.label, c.menu, c.enabled))
        .collect();
    assert_eq!(
        row,
        [
            ("Move & attach…", "Move & attach…", true),
            ("Coordinate in place…", "Coordinate in place…", true)
        ]
    );
    let row_labels: Vec<_> = app.context_commands().iter().map(|c| c.label).collect();
    assert!(row_labels.contains(&"Coordinate in place…"));
    assert!(
        row_labels.contains(&"Coordinate }"),
        "Keyboard drawing keeps its own command"
    );
    app.tab.selected.clear();
    assert!(app.selected_attach_commands().iter().all(|c| !c.enabled));
}
