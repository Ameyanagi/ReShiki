use super::*;

fn chain() -> (Document, u64, u64, u64) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("N", Point::new(42., -21.));
    let c = doc.add_atom("O", Point::new(84., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    (doc, a, b, c)
}

#[test]
fn arrows_traverse_hotspots_and_shift_skips_without_editing() {
    let (doc, a, b, c) = chain();
    let before = doc.clone();
    let bond = Target::Bond(a, b);
    assert_eq!(
        navigate(&doc, Target::Atom(a), Direction::Right, false),
        Some(bond)
    );
    assert_eq!(
        navigate(&doc, bond, Direction::Right, false),
        Some(Target::Atom(b))
    );
    assert_eq!(
        navigate(&doc, Target::Atom(b), Direction::Left, false),
        Some(bond)
    );
    assert_eq!(
        navigate(&doc, Target::Atom(a), Direction::Right, true),
        Some(Target::Atom(b))
    );
    assert_eq!(
        navigate(&doc, bond, Direction::Right, true),
        Some(Target::Bond(b, c))
    );
    assert_eq!(
        navigate(&doc, Target::Atom(a), Direction::Left, false),
        None
    );
    assert_eq!(doc, before);
}

#[test]
fn geometry_ties_are_stable_and_connected_targets_win() {
    let (mut doc, a, b, _) = chain();
    let nearby = doc.add_atom("C", Point::new(1., 0.));
    let far = doc.add_atom("C", Point::new(2., 0.));
    doc.add_bond(nearby, far, 1, "plain");
    assert_eq!(
        navigate(&doc, Target::Atom(a), Direction::Right, false),
        Some(Target::Bond(a, b))
    );
    let expected = navigate(
        &doc,
        Target::Blank(Point::new(-1., 0.)),
        Direction::Right,
        false,
    );
    doc.atoms.reverse();
    doc.bonds.reverse();
    assert_eq!(
        navigate(
            &doc,
            Target::Blank(Point::new(-1., 0.)),
            Direction::Right,
            false
        ),
        expected
    );
}

#[test]
fn cursor_history_restores_deleted_targets_and_resets_at_file_boundaries() {
    let (before, a, b, _) = chain();
    let mut state = State::default();
    state.enter(&before, &[b], Point::default(), 7);
    state.mark(a);
    let mut after = before.clone();
    after.atoms.retain(|atom| atom.id != b);
    after.bonds.retain(|bond| bond.a != b && bond.b != b);
    state.record(&before, &after, 7, false);
    assert!(matches!(state.target(), Target::Blank(_)));
    state.restore(false, &before, 7);
    assert_eq!(state.target(), Target::Atom(b));
    assert_eq!(state.marked(), Some(a));
    state.restore(true, &after, 7);
    assert!(matches!(state.target(), Target::Blank(_)));
    state.reconcile(&before, 8);
    assert!(state.enabled());
    assert_eq!(state.marked(), None);
}

#[test]
fn pointer_handoff_requires_motion_or_click_and_opt_out_survives_tool_suspension() {
    let (doc, a, b, _) = chain();
    let mut state = State::default();
    assert!(state.enabled());
    state.ensure_target(&doc, &[a], Point::default(), 0);
    let point = doc.atom(a).unwrap().position;
    assert!(state.pointer_target(point, Target::Atom(a), &doc, false));
    assert!(state.move_target(&doc, Direction::Right, false));
    assert_eq!(state.target(), Target::Bond(a, b));
    assert!(!state.pointer_target(point, Target::Atom(a), &doc, false));
    assert_eq!(state.target(), Target::Bond(a, b));
    assert!(state.pointer_target(point, Target::Atom(a), &doc, true));
    assert_eq!(state.target(), Target::Atom(a));
    state.suspend();
    assert!(state.enabled());
    assert!(!state.active());
    state.ensure_target(&doc, &[b], Point::default(), 0);
    assert!(state.active());
    assert_eq!(state.target(), Target::Atom(b));
    state.leave();
    state.suspend();
    state.ensure_target(&doc, &[a], Point::default(), 0);
    assert!(!state.active());
    assert!(!state.enabled());
    state.reconcile(&doc, 1);
    assert!(
        !state.enabled(),
        "File epoch changes must not undo the explicit preference"
    );
}
