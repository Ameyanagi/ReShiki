//! State-level regressions for bracket annotations and explicit depiction edits.
use super::*;
use reshiki::graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle};

fn ready(document: Document, selected: Vec<u64>) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = document;
    app.tab.saved = app.tab.doc.clone();
    app.set_selection(selected);
    app
}

fn charges(document: &Document) -> Vec<(u64, i32)> {
    document
        .atoms
        .iter()
        .map(|atom| (atom.id, atom.charge))
        .collect()
}

fn carbonate() -> (Document, Vec<u64>) {
    let mut document = Document::default();
    let carbon = document.add_atom("C", Point::default());
    let oxygen = document.add_atom("O", Point::new(0., -42.));
    let left = document.add_atom("O", Point::new(-36., 21.));
    let right = document.add_atom("O", Point::new(36., 21.));
    document.add_bond(carbon, oxygen, 2, "plain");
    document.add_bond(carbon, left, 1, "plain");
    document.add_bond(carbon, right, 1, "plain");
    document.atom_mut(left).unwrap().charge = -1;
    document.atom_mut(right).unwrap().charge = -1;
    (document, vec![carbon, oxygen, left, right])
}

fn assert_one_undo_redo(app: &mut App, before: &Document) {
    assert!(!app.error, "{}", app.status);
    let after = app.tab.doc.clone();
    assert_ne!(&after, before);
    let _ = app.update(Message::Undo);
    assert_eq!(&app.tab.doc, before, "one undo restores the complete edit");
    assert!(
        !app.tab.history.can_undo(),
        "the edit must create one history frame"
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}

#[test]
fn bracket_ion_groups_carbonate_and_hides_local_charges_in_one_undo_step() {
    let (mut document, selected) = carbonate();
    let spectator = document.add_atom("Na", Point::new(200., 0.));
    document.atom_mut(spectator).unwrap().charge = 1;
    let mut app = ready(document, selected.clone());
    let before = app.tab.doc.clone();
    let _ = app.update(Message::BracketIon);
    assert!(!app.error, "{}", app.status);
    assert_eq!(charges(&app.tab.doc), charges(&before));
    assert_eq!(app.tab.doc.bonds, before.bonds);
    assert!(!chemistry_changed(&before, &app.tab.doc));
    for atom in &app.tab.doc.atoms {
        assert_eq!(atom.display.hide_charge, selected.contains(&atom.id));
    }
    assert_eq!(app.tab.doc.graphics.len(), 1);
    let bracket = &app.tab.doc.graphics[0];
    assert_eq!(bracket.kind, GraphicKind::Brackets);
    assert_eq!(bracket.sides, BracketSides::Both);
    assert_eq!(bracket.superscript.as_deref(), Some("2−"));
    let mut members = selected.clone();
    members.push(bracket.id);
    members.sort_unstable();
    assert_eq!(app.tab.doc.groups.len(), 1);
    let mut grouped = app.tab.doc.groups[0].members.clone();
    grouped.sort_unstable();
    assert_eq!(grouped, members);
    let mut selected_after = app.tab.selected.clone();
    selected_after.sort_unstable();
    assert_eq!(selected_after, members);
    assert_eq!(app.tab.bracket_superscript, graphics::Superscript::Custom);
    assert_eq!(app.tab.bracket_superscript_input, "2−");
    assert_one_undo_redo(&mut app, &before);

    let bracketed = app.tab.doc.clone();
    let _ = app.update(Message::Labels(atom_labels::Action::Charges(true)));
    assert!(
        app.tab
            .doc
            .atoms
            .iter()
            .all(|atom| !atom.display.hide_charge)
    );
    assert_eq!(charges(&app.tab.doc), charges(&before));
    assert_eq!(app.tab.doc.graphics, bracketed.graphics);
    assert_eq!(app.tab.doc.groups, bracketed.groups);
    let visible = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.tab.doc, bracketed,
        "showing local labels is a separate undo step"
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, visible);
}

#[test]
fn invalid_ion_selections_leave_the_document_and_history_untouched() {
    for case in ["empty", "partial", "disconnected", "neutral"] {
        let (mut document, mut selected) = carbonate();
        match case {
            "empty" => selected.clear(),
            "partial" => {
                selected.pop();
            }
            "disconnected" => selected.push(document.add_atom("Na", Point::new(200., 0.))),
            "neutral" => {
                for atom in &mut document.atoms {
                    atom.charge = 0;
                }
            }
            _ => unreachable!(),
        }
        let mut app = ready(document, selected.clone());
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        let _ = app.update(Message::BracketIon);
        assert!(app.error, "{case}");
        assert_eq!(app.tab.doc, before, "{case}");
        assert_eq!(app.tab.selected, selected, "{case}");
        assert_eq!(app.tab.revision, revision, "{case}");
        assert!(!app.tab.history.can_undo(), "{case}");
        assert!(!app.tab.history.can_redo(), "{case}");
    }
}

#[derive(Clone, Copy)]
struct Nitro {
    nitrogen: u64,
    oxygens: [u64; 2],
}

fn add_nitro(document: &mut Document, x: f32) -> Nitro {
    let nitrogen = document.add_atom("N", Point::new(x, 0.));
    let carbon = document.add_atom("C", Point::new(x - 42., 0.));
    let first = document.add_atom("O", Point::new(x + 30., -30.));
    let second = document.add_atom("O", Point::new(x + 30., 30.));
    document.add_bond(nitrogen, carbon, 1, "plain");
    document.add_bond(nitrogen, first, 2, "plain");
    document.add_bond(nitrogen, second, 2, "plain");
    Nitro {
        nitrogen,
        oxygens: [first, second],
    }
}

fn assert_nitro(document: &Document, center: Nitro, separated: bool) {
    assert_eq!(
        document.atom(center.nitrogen).unwrap().charge,
        i32::from(separated)
    );
    let mut oxygen_states: Vec<_> = center
        .oxygens
        .iter()
        .map(|&id| {
            (
                document.bond_between(center.nitrogen, id).unwrap().1.order,
                document.atom(id).unwrap().charge,
            )
        })
        .collect();
    oxygen_states.sort_unstable();
    assert_eq!(
        oxygen_states,
        if separated {
            vec![(1, -1), (2, 0)]
        } else {
            vec![(2, 0), (2, 0)]
        }
    );
}

#[test]
fn normalize_depiction_limits_selected_centers_and_normalizes_the_whole_drawing_when_empty() {
    for selected_only in [true, false] {
        let mut document = Document::default();
        let first = add_nitro(&mut document, 0.);
        let second = add_nitro(&mut document, 200.);
        let selected = if selected_only {
            vec![first.nitrogen]
        } else {
            vec![]
        };
        let mut app = ready(document, selected.clone());
        let before = app.tab.doc.clone();
        let _ = app.update(Message::NormalizeDepiction);
        assert!(!app.error, "{}", app.status);
        assert_nitro(&app.tab.doc, first, true);
        assert_nitro(&app.tab.doc, second, !selected_only);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.tab.doc.atoms.len(), before.atoms.len());
        assert_eq!(app.tab.doc.bonds.len(), before.bonds.len());
        assert_eq!(app.tab.doc.atoms.iter().map(|a| a.charge).sum::<i32>(), 0);
        for atom in &before.atoms {
            assert_eq!(app.tab.doc.atom(atom.id).unwrap().position, atom.position);
        }
        assert_one_undo_redo(&mut app, &before);
    }
}

#[test]
fn normalize_depiction_no_ops_preserve_redo_and_do_not_add_history() {
    let mut document = Document::default();
    let center = add_nitro(&mut document, 0.);
    let mut app = ready(document, vec![]);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::NormalizeDepiction);
    let separated = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    app.set_selection(vec![center.oxygens[0]]);
    let revision = app.tab.revision;
    let _ = app.update(Message::NormalizeDepiction);
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.tab.doc, before,
        "selecting an oxygen does not normalize its unselected N"
    );
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.history.can_redo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, separated);
    app.set_selection(vec![]);
    let revision = app.tab.revision;
    let _ = app.update(Message::NormalizeDepiction);
    assert_eq!(app.tab.doc, separated, "normalization is idempotent");
    assert_eq!(app.tab.revision, revision);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn charge_separated_context_action_uses_explicit_targets_in_one_undo_step() {
    let mut document = Document::default();
    let unselected = add_nitro(&mut document, 0.);
    let target = add_nitro(&mut document, 200.);
    let mut app = ready(document, vec![unselected.nitrogen]);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::DrawChargeSeparated(vec![target.nitrogen]));
    assert_nitro(&app.tab.doc, unselected, false);
    assert_nitro(&app.tab.doc, target, true);
    assert_eq!(app.tab.selected, [unselected.nitrogen]);
    assert_one_undo_redo(&mut app, &before);
}

fn shape(id: u64, kind: GraphicKind) -> Graphic {
    Graphic::dragged(
        id,
        kind,
        Point::new(id as f32 * 100., 0.),
        Point::new(id as f32 * 100. + 70., 40.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    )
}

fn bracket_selection() -> App {
    let mut document = Document {
        graphics: vec![
            shape(1, GraphicKind::Brackets),
            shape(2, GraphicKind::Parentheses),
            shape(3, GraphicKind::Braces),
            shape(4, GraphicKind::Rectangle),
            shape(5, GraphicKind::Brackets),
        ],
        ..Default::default()
    };
    document.graphics[4].superscript = Some("unchanged".into());
    let atom = document.add_atom("N", Point::new(0., 20.));
    document.atom_mut(atom).unwrap().charge = 1;
    ready(document, vec![1, 2, 3, 4])
}

fn assert_bracket_superscripts(app: &App, expected: Option<&str>) {
    for bracket in &app.tab.doc.graphics[..3] {
        assert_eq!(bracket.superscript.as_deref(), expected);
    }
    assert_eq!(app.tab.doc.graphics[3].superscript, None);
    assert_eq!(
        app.tab.doc.graphics[4].superscript.as_deref(),
        Some("unchanged")
    );
}

#[test]
fn superscript_presets_apply_to_all_selected_bracket_kinds_in_one_undo_step() {
    use graphics::{Action, Superscript};
    for (preset, expected) in [
        (Superscript::TransitionState, "‡"),
        (Superscript::Positive, "+"),
        (Superscript::Negative, "−"),
        (Superscript::RadicalPositive, "•+"),
        (Superscript::RadicalNegative, "•−"),
    ] {
        let mut app = bracket_selection();
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Graphics(Action::Superscript(preset)));
        assert_bracket_superscripts(&app, Some(expected));
        assert_eq!(app.tab.doc.atoms, before.atoms);
        assert!(!chemistry_changed(&before, &app.tab.doc));
        assert_one_undo_redo(&mut app, &before);
        let annotated = app.tab.doc.clone();
        let _ = app.update(Message::Graphics(Action::Superscript(Superscript::None)));
        assert_bracket_superscripts(&app, None);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, annotated);
    }
}

#[test]
fn custom_superscript_drafts_preserve_the_document_and_redo_until_apply() {
    use graphics::{Action, Superscript};
    let mut app = bracket_selection();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Graphics(Action::Superscript(
        Superscript::Positive,
    )));
    let _ = app.update(Message::Undo);
    let revision = app.tab.revision;
    let _ = app.update(Message::Graphics(Action::Superscript(Superscript::Custom)));
    let _ = app.update(Message::Graphics(Action::SuperscriptText("2−".into())));
    assert_eq!(app.tab.bracket_superscript_input, "2−");
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.history.can_redo());
    let _ = app.update(Message::Graphics(Action::ApplySuperscript));
    assert_bracket_superscripts(&app, Some("2−"));
    assert_eq!(app.tab.doc.atoms, before.atoms);
    assert!(!app.tab.history.can_redo());
    assert_one_undo_redo(&mut app, &before);
}

#[test]
fn mixed_selection_syncs_superscript_from_a_bracket_after_a_rectangle() {
    use graphics::{Action, Superscript};
    let mut document = Document::default();
    let rectangle = shape(1, GraphicKind::Rectangle);
    let mut bracket = shape(2, GraphicKind::Brackets);
    bracket.superscript = Some("2−".into());
    document.graphics = vec![rectangle.clone(), bracket];
    let mut app = ready(document, vec![]);
    app.set_selection(vec![1, 2]);
    assert_eq!(app.tab.bracket_superscript, Superscript::Custom);
    assert_eq!(app.tab.bracket_superscript_input, "2−");
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Graphics(Action::Superscript(
        Superscript::TransitionState,
    )));
    assert_eq!(app.tab.doc.graphics[0], rectangle);
    assert_eq!(app.tab.doc.graphics[1].superscript.as_deref(), Some("‡"));
    assert_one_undo_redo(&mut app, &before);
}

#[test]
fn transition_state_palette_preset_draws_the_annotation_and_plain_preset_clears_it() {
    use palettes::{Action, Family, graphic_options};
    let mut app = ready(Document::default(), vec![]);
    let transition = graphic_options(Family::Brackets)
        .into_iter()
        .find(|(name, _)| name == "Transition state [ ]‡")
        .unwrap()
        .1;
    assert_eq!(transition.kind, GraphicKind::Brackets);
    assert_eq!(transition.sides, BracketSides::Both);
    assert_eq!(transition.superscript.as_deref(), Some("‡"));
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Palette(Action::Graphic(transition)));
    assert_eq!(
        app.tab.bracket_superscript,
        graphics::Superscript::TransitionState
    );
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    app.edit(Edit::Graphic(Point::default(), Point::new(90., 60.), false));
    assert_eq!(app.tab.doc.graphics[0].superscript.as_deref(), Some("‡"));
    assert_one_undo_redo(&mut app, &before);

    let plain = graphic_options(Family::Brackets)
        .into_iter()
        .find(|(_, option)| {
            option.kind == GraphicKind::Brackets
                && option.sides == BracketSides::Both
                && option.superscript.is_none()
        })
        .unwrap()
        .1;
    let _ = app.update(Message::Palette(Action::Graphic(plain)));
    assert_eq!(app.tab.bracket_superscript, graphics::Superscript::None);
    app.edit(Edit::Graphic(
        Point::new(150., 0.),
        Point::new(240., 60.),
        false,
    ));
    assert_eq!(app.tab.doc.graphics.len(), 2);
    assert_eq!(app.tab.doc.graphics[0].superscript.as_deref(), Some("‡"));
    assert_eq!(app.tab.doc.graphics[1].superscript, None);
}
