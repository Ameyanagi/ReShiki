//! Caption-to-bond edits must commit the conversion and bond in one transaction.
use super::*;
use reshiki::document::{Annotation, Document, History};

fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    app.tab.history = History::default();
    app.tab.busy = false;
    app.tab.bond_drawing.length = 42.;
    app.tool = Tool::Bond(1);
    app
}

fn caption(doc: &mut Document, text: &str, position: Point) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        position,
        text: text.into(),
        format: Default::default(),
    });
    id
}

fn annotation(doc: &Document, id: u64) -> &Annotation {
    doc.annotations.iter().find(|a| a.id == id).unwrap()
}

/// Where a convertible caption's atom lands (inside its ink); a refused
/// caption has no atom, so use its first glyph as the pointer position.
fn ink(doc: &Document, id: u64) -> Point {
    reshiki::atom_text::text_atom_position(doc, id)
        .or_else(|| reshiki::atom_text::text_first_glyph_center(annotation(doc, id)))
        .unwrap()
}

fn source_edit(doc: &Document, id: u64, end: Option<Point>) -> Edit {
    Edit::TextBond {
        source: None,
        source_text: Some(id),
        start: ink(doc, id),
        end,
        target: None,
        target_text: None,
    }
}

fn molecule_caption(doc: &mut Document) -> u64 {
    let carbon = doc.add_atom("C", Point::new(-180., -80.));
    reshiki::molecule_names::show(
        doc,
        &[carbon],
        "methane".into(),
        "C".into(),
        Default::default(),
    )
    .unwrap()
}

fn assert_unchanged_failure(app: &App, before: &Document, revision: u64, selected: &[u64]) {
    assert_eq!(&app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.selected, selected);
    assert!(!app.tab.history.can_undo());
    assert!(app.error, "{}", app.status);
}

#[test]
fn pd_caption_drag_keeps_the_first_glyph_in_place_and_bonds_in_one_undo_step() {
    let mut app = app();
    let text = caption(&mut app.tab.doc, "Pd2+", Point::new(15., 20.));
    // The model aligns the label's first glyph on the caption's first glyph.
    let expected = ink(&app.tab.doc, text);
    let (lo, hi) = reshiki::atom_text::text_ink_bounds(annotation(&app.tab.doc, text)).unwrap();
    assert!((lo.x..=hi.x).contains(&expected.x) && (lo.y..=hi.y).contains(&expected.y));
    let before = app.tab.doc.clone();
    let end = expected.offset(42., 0.);
    app.edit(source_edit(&before, text, Some(end)));

    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Bond from text");
    assert!(app.tab.doc.annotations.is_empty());
    assert_eq!(app.tab.doc.atoms.len(), 2);
    let pd = app
        .tab
        .doc
        .atoms
        .iter()
        .find(|a| a.element == "Pd")
        .unwrap();
    assert_eq!(pd.charge, 2);
    assert_eq!(pd.position, expected);
    let carbon = app.tab.doc.atoms.iter().find(|a| a.element == "C").unwrap();
    assert_eq!(carbon.position, end);
    assert_eq!(app.tab.doc.bonds.len(), 1);
    assert_eq!(app.tab.doc.bonds[0].order, 1);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    app.step_history(false);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn caption_click_grows_the_default_bond_in_one_undo_step() {
    let mut app = app();
    let text = caption(&mut app.tab.doc, "Pd2+", Point::new(-25., -10.));
    let start = ink(&app.tab.doc, text);
    let before = app.tab.doc.clone();
    app.edit(source_edit(&before, text, None));

    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Bond from text");
    assert!(app.tab.doc.annotations.is_empty());
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (2, 1));
    let pd = app
        .tab
        .doc
        .atoms
        .iter()
        .find(|a| a.element == "Pd")
        .unwrap();
    let carbon = app.tab.doc.atoms.iter().find(|a| a.element == "C").unwrap();
    assert_eq!((pd.position, pd.charge), (start, 2));
    assert!((carbon.position.distance(start) - 42.).abs() < 0.001);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    app.step_history(false);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn nc_caption_is_read_from_the_bond_drawn_from_or_to_it() {
    // Dragged out to the right, the bond meets the C of NC: a nitrile.
    let mut app = app();
    let text = caption(&mut app.tab.doc, "NC", Point::new(15., 20.));
    let before = app.tab.doc.clone();
    let start = ink(&before, text);
    app.edit(source_edit(&before, text, Some(start.offset(42., 0.))));
    assert!(!app.error, "{}", app.status);
    let group = &app.tab.doc.abbreviations[0];
    assert_eq!(
        (group.label.as_str(), group.reverse_label.as_str()),
        ("CN", "NC")
    );
    assert!(
        app.tab
            .doc
            .bonds
            .iter()
            .any(|b| b.a == group.anchor && !group.members.contains(&b.b))
    );
    // Drawn into from the left, it meets the N: isocyanide text.
    let mut app = self::app();
    let source = app.tab.doc.add_atom("C", Point::new(-60., 0.));
    let text = caption(&mut app.tab.doc, "NC", Point::new(25., -10.));
    let before = app.tab.doc.clone();
    app.edit(Edit::TextBond {
        source: Some(source),
        source_text: None,
        start: before.atom(source).unwrap().position,
        end: Some(ink(&before, text)),
        target: None,
        target_text: Some(text),
    });
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.doc.abbreviations.is_empty());
    assert!(
        app.tab
            .doc
            .atoms
            .iter()
            .any(|a| a.display.variable.as_deref() == Some("NC"))
    );
}

#[test]
fn atom_drag_to_caption_converts_only_the_target_and_undo_restores_it() {
    let mut app = app();
    let source = app.tab.doc.add_atom("O", Point::new(-60., 0.));
    let text = caption(&mut app.tab.doc, "Pd2+", Point::new(25., -10.));
    let target = ink(&app.tab.doc, text);
    let before = app.tab.doc.clone();
    app.edit(Edit::TextBond {
        source: Some(source),
        source_text: None,
        start: before.atom(source).unwrap().position,
        end: Some(target),
        target: None,
        target_text: Some(text),
    });

    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Bond from text");
    assert_eq!(app.tab.doc.atom(source), before.atom(source));
    assert!(app.tab.doc.annotations.is_empty());
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (2, 1));
    let pd = app
        .tab
        .doc
        .atoms
        .iter()
        .find(|a| a.element == "Pd")
        .unwrap();
    assert_eq!((pd.position, pd.charge), (target, 2));
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    app.step_history(false);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn caption_drag_to_caption_converts_both_ends_in_one_undo_step() {
    let mut app = app();
    let source = caption(&mut app.tab.doc, "Pd2+", Point::new(-80., 20.));
    let target = caption(&mut app.tab.doc, "O", Point::new(80., 20.));
    let start = ink(&app.tab.doc, source);
    let end = ink(&app.tab.doc, target);
    let before = app.tab.doc.clone();
    app.edit(Edit::TextBond {
        source: None,
        source_text: Some(source),
        start,
        end: Some(end),
        target: None,
        target_text: Some(target),
    });

    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Bond from text");
    assert!(app.tab.doc.annotations.is_empty());
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (2, 1));
    let pd = app
        .tab
        .doc
        .atoms
        .iter()
        .find(|a| a.element == "Pd")
        .unwrap();
    let oxygen = app.tab.doc.atoms.iter().find(|a| a.element == "O").unwrap();
    assert_eq!((pd.position, pd.charge), (start, 2));
    assert_eq!(oxygen.position, end);
    let bond = &app.tab.doc.bonds[0];
    assert_eq!((bond.a, bond.b, bond.order), (pd.id, oxygen.id, 1));
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    app.step_history(false);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn overlong_caption_refuses_bond_without_document_or_history_changes() {
    let mut app = app();
    let text = caption(&mut app.tab.doc, &"x".repeat(40), Point::default());
    app.tab.selected = vec![text];
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    app.edit(source_edit(
        &before,
        text,
        Some(ink(&before, text).offset(42., 0.)),
    ));
    assert_unchanged_failure(&app, &before, revision, &[text]);
    assert_eq!(app.status, reshiki::atom_text::TEXT_TOO_LONG);
}

#[test]
fn molecule_name_caption_refuses_bond_without_document_or_history_changes() {
    let mut app = app();
    let text = molecule_caption(&mut app.tab.doc);
    app.tab.selected = vec![text];
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    app.edit(source_edit(&before, text, None));
    assert_unchanged_failure(&app, &before, revision, &[text]);
    assert_eq!(
        app.status,
        "This text is a molecule name; it stays a caption"
    );
}

#[test]
fn reaction_caption_refuses_bond_without_document_or_history_changes() {
    let mut app = app();
    let text = caption(&mut app.tab.doc, "Pd2+", Point::default());
    let carbon = app.tab.doc.add_atom("C", Point::new(-180., -80.));
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        Point::new(-100., -80.),
        Point::new(-20., -80.),
        Default::default(),
        Default::default(),
    ));
    reshiki::reactions::assign(
        &mut app.tab.doc,
        arrow,
        &[carbon],
        reshiki::reactions::Role::Reactant,
    )
    .unwrap();
    app.tab.doc.reactions[0].annotations.push(text);
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    app.edit(source_edit(&before, text, None));
    assert_unchanged_failure(&app, &before, revision, &[]);
    assert_eq!(
        app.status,
        "This text belongs to a reaction; it stays a caption"
    );
}

#[test]
fn refused_target_does_not_commit_the_successful_source_conversion() {
    for named_target in [false, true] {
        let mut app = app();
        let source = caption(&mut app.tab.doc, "Pd2+", Point::new(40., 30.));
        let target = if named_target {
            molecule_caption(&mut app.tab.doc)
        } else {
            caption(&mut app.tab.doc, &"x".repeat(40), Point::new(180., 30.))
        };
        app.tab.selected = vec![source];
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        app.edit(Edit::TextBond {
            source: None,
            source_text: Some(source),
            start: ink(&before, source),
            end: Some(ink(&before, target)),
            target: None,
            target_text: Some(target),
        });
        assert_unchanged_failure(&app, &before, revision, &[source]);
        assert_eq!(app.tab.doc.annotations.len(), 2);
    }
}

#[test]
fn refusal_edit_sets_error_status_without_document_or_history_changes() {
    let mut app = app();
    let before = app.tab.doc.clone();
    app.edit(Edit::TextBondRefused(
        reshiki::atom_text::TEXT_TOO_LONG.into(),
    ));
    assert_unchanged_failure(&app, &before, 0, &[]);
    assert_eq!(app.status, reshiki::atom_text::TEXT_TOO_LONG);
}

#[test]
fn refused_canvas_drop_keeps_document_and_history_and_shows_one_refusal() {
    for named in [false, true] {
        let mut app = app();
        let source = app.tab.doc.add_atom("C", Point::new(-180., -70.));
        let start = app.tab.doc.atom(source).unwrap().position;
        let label = if named {
            "methane".into()
        } else {
            "x".repeat(40)
        };
        let text = caption(&mut app.tab.doc, &label, Point::new(80., 10.));
        if named {
            app.tab
                .doc
                .molecule_names
                .push(reshiki::molecule_names::MoleculeName {
                    annotation: text,
                    atoms: vec![source],
                    smiles: "C".into(),
                    text: "methane".into(),
                });
        }
        let cursor = ink(&app.tab.doc, text);
        app.tab.selected = vec![source];
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        let reason = reshiki::atom_text::text_atom_refusal(&before, text).unwrap();
        let edits = crate::canvas::bond_text_drag_edits(&before, app.tool, start, cursor);
        let mut refusals = 0;
        for edit in edits {
            refusals += usize::from(matches!(edit, Edit::TextBondRefused(_)));
            let hover = matches!(edit, Edit::Hover(Some(p)) if p.distance(cursor) < 0.001);
            app.edit(edit);
            if hover {
                assert_eq!(app.status, reason);
                assert!(!app.error, "Hover is a hint before release");
            }
        }
        assert_eq!(refusals, 1);
        assert_unchanged_failure(&app, &before, revision, &[source]);
        assert_eq!(app.status, reason);
        assert_eq!(app.tab.history.undo_frames().len(), 0);
        assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (1, 0));
    }
}

#[test]
fn bond_caption_hover_restores_the_previous_status_and_error_on_leave() {
    for leave in [None, Some(Point::new(350., 220.))] {
        let mut app = app();
        let text = caption(&mut app.tab.doc, "Pd2+", Point::default());
        let point = ink(&app.tab.doc, text);
        let before = app.tab.doc.clone();
        app.status = "Previous structure warning".into();
        app.error = true;
        app.edit(Edit::Hover(Some(point)));
        assert_eq!(app.status, BOND_TEXT_HINT);
        assert!(!app.error);
        app.edit(Edit::Hover(leave));
        assert_eq!(app.status, "Previous structure warning");
        assert!(app.error);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn molecule_caption_hover_shows_a_refusal_hint_without_an_error() {
    let mut app = app();
    let text = molecule_caption(&mut app.tab.doc);
    let point = ink(&app.tab.doc, text);
    let before = app.tab.doc.clone();
    app.status = "Previous warning".into();
    app.error = true;
    app.edit(Edit::Hover(Some(point)));
    assert_eq!(
        app.status,
        "This text is a molecule name; it stays a caption"
    );
    assert!(!app.error);
    app.edit(Edit::Hover(None));
    assert_eq!(app.status, "Previous warning");
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn select_caption_hover_does_not_show_the_bond_hint() {
    let mut app = app();
    let text = caption(&mut app.tab.doc, "Pd2+", Point::default());
    let point = ink(&app.tab.doc, text);
    app.tool = Tool::Select;
    app.status = "Selection status".into();
    app.error = true;
    app.edit(Edit::Hover(Some(point)));
    assert_eq!(app.status, "Selection status");
    assert!(app.error);
    app.edit(Edit::Hover(None));
    assert_eq!(app.status, "Selection status");
    assert!(app.error);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn switching_to_select_while_hovering_restores_the_previous_status_and_error() {
    let mut app = app();
    let text = caption(&mut app.tab.doc, "Pd2+", Point::default());
    let point = ink(&app.tab.doc, text);
    app.status = "Previous structure warning".into();
    app.error = true;
    app.edit(Edit::Hover(Some(point)));
    assert_eq!(app.status, BOND_TEXT_HINT);
    assert!(!app.error);
    // The tool change itself restores the status, then clears the error flag
    // as it does without a hover.
    app.select_tool(Tool::Select);
    assert_eq!(app.status, "Previous structure warning");
    assert!(!app.error);
    app.edit(Edit::Hover(Some(point)));
    assert_eq!(app.status, "Previous structure warning");
    app.edit(Edit::Hover(None));
    assert_eq!(app.status, "Previous structure warning");
    assert!(!app.error);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn switching_tabs_while_hovering_parks_the_previous_status_and_error() {
    let mut app = app();
    let text = caption(&mut app.tab.doc, "Pd2+", Point::default());
    let point = ink(&app.tab.doc, text);
    app.status = "Previous structure warning".into();
    app.error = true;
    app.edit(Edit::Hover(Some(point)));
    assert_eq!(app.status, BOND_TEXT_HINT);
    app.add_tab();
    assert_ne!(app.status, BOND_TEXT_HINT);
    app.status = "Second tab status".into();
    app.error = false;
    app.edit(Edit::Hover(Some(Point::new(350., 220.))));
    assert_eq!(app.status, "Second tab status");
    app.select_tab(0);
    assert_eq!(app.status, "Previous structure warning");
    assert!(app.error);
    app.select_tab(1);
    assert_eq!(app.status, "Second tab status");
    assert!(!app.error);
}

#[test]
fn refusal_error_matching_the_hover_hint_wins_when_the_pointer_leaves() {
    let mut app = app();
    let text = molecule_caption(&mut app.tab.doc);
    let point = ink(&app.tab.doc, text);
    let reason = reshiki::atom_text::text_atom_refusal(&app.tab.doc, text).unwrap();
    app.status = "Before hover".into();
    app.edit(Edit::Hover(Some(point)));
    assert_eq!(app.status, reason);
    assert!(!app.error);
    app.edit(Edit::TextBondRefused(reason.clone()));
    assert_eq!(app.status, reason);
    assert!(app.error);
    app.edit(Edit::Hover(None));
    assert_eq!(app.status, reason);
    assert!(app.error);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn status_written_during_caption_hover_wins_when_the_pointer_leaves() {
    let mut app = app();
    let text = caption(&mut app.tab.doc, "Pd2+", Point::default());
    let point = ink(&app.tab.doc, text);
    app.status = "Before hover".into();
    app.error = true;
    app.edit(Edit::Hover(Some(point)));
    assert_eq!(app.status, BOND_TEXT_HINT);
    app.status = "A newer operation failed".into();
    app.error = true;
    app.edit(Edit::Hover(None));
    assert_eq!(app.status, "A newer operation failed");
    assert!(app.error);
}

#[test]
fn caption_chain_conversion_and_all_bonds_are_one_undo_step() {
    let mut app = app();
    app.tool = Tool::Chain(reshiki::chains::ChainMode::Straight);
    let text = caption(&mut app.tab.doc, "Pd2+", Point::new(15., 20.));
    let start = ink(&app.tab.doc, text);
    let before = app.tab.doc.clone();
    app.edit(Edit::Chain {
        points: vec![start, start.offset(42., 0.), start.offset(84., 0.)],
        source: None,
        source_text: Some(text),
        target: None,
    });

    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Bond from text");
    assert!(app.tab.doc.annotations.is_empty());
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (3, 2));
    let pd = app
        .tab
        .doc
        .atoms
        .iter()
        .find(|a| a.element == "Pd")
        .unwrap();
    assert_eq!((pd.position, pd.charge), (start, 2));
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    app.step_history(false);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn symbol_placements_repeat_until_escape_and_undo_individually() {
    use reshiki::{graphics::GraphicKind, scientific::SymbolKind};
    let mut app = app();
    let oxygen = app.tab.doc.add_atom("O", Point::default());
    let tool = Tool::Graphic(GraphicKind::Symbol(SymbolKind::LonePair));
    app.tool = tool;
    for count in 1..=2 {
        let before = app.tab.doc.clone();
        app.place_graphic(Point::default(), Point::default(), false, true, before);
        assert_eq!(app.tool, tool);
        assert_eq!(app.tab.doc.atom(oxygen).unwrap().marks.len(), count);
    }
    assert_eq!(app.tab.history.undo_frames().len(), 2);
    app.step_history(false);
    assert_eq!(app.tab.doc.atom(oxygen).unwrap().marks.len(), 1);
    let _ = app.update(crate::app::Message::Escape);
    assert_eq!(app.tool, Tool::Select);
}
