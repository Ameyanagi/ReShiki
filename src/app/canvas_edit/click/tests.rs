use super::*;
use reshiki::typography::{Script, ScriptRun};

#[test]
fn atom_palette_replacement_clears_manual_label_runs() {
    let (mut app, _) = App::new();
    let mut source = Document::default();
    let id = source.add_atom("*", Point::default());
    source = reshiki::atom_text::apply_with_scripts(
        &source,
        id,
        "M2+",
        &[ScriptRun {
            start: 1,
            end: 3,
            script: Script::Superscript,
        }],
        reshiki::atom_text::Mode::Text,
    )
    .unwrap();
    app.tab.doc = source.clone();
    app.choose_element("N".into());
    app.canvas_click(Point::default(), source);
    let atom = app.tab.doc.atom(id).unwrap();
    assert_eq!(atom.element, "N");
    assert!(atom.display.variable.is_none());
    assert!(atom.display.label_scripts.is_empty());
    app.tab.doc.validate().unwrap();
}

fn backward_chain() -> (Document, u64, u64) {
    let mut doc = Document::default();
    let left = doc.add_atom("C", Point::default());
    let center = doc.add_atom("C", Point::new(84., 0.));
    let tail = doc.add_atom("C", Point::new(-84., 0.));
    let fluorine = doc.add_atom("F", Point::new(84., -84.));
    let chlorine = doc.add_atom("Cl", Point::new(84., 84.));
    doc.add_bond(left, center, 1, "plain");
    doc.add_bond(left, tail, 1, "plain");
    doc.add_bond(center, fluorine, 1, "plain");
    doc.add_bond(center, chlorine, 1, "plain");
    (doc, left, center)
}

#[test]
fn first_wedge_click_matches_preview_then_second_click_reverses_and_undo_restores() {
    use reshiki::bonds::BondPreset as P;
    for tool in [
        Tool::Wedge,
        Tool::Hash,
        Tool::StyledBond(P::Wedge),
        Tool::StyledBond(P::HashedWedge),
        Tool::StyledBond(P::HollowWedge),
    ] {
        let (mut app, _) = App::new();
        let (source, left, center) = backward_chain();
        app.tab.doc = source.clone();
        app.tool = tool;
        let preset = tool.bond_preset().unwrap();
        let mut preview = source.clone();
        // The drag preview uses this same placement on its drawing copy.
        preset.place(&mut preview, left, center);
        app.canvas_click(Point::new(42., 0.), source);
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.bonds, preview.bonds);
        assert_eq!(
            (app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b),
            (center, left)
        );
        let first_click = app.tab.doc.clone();
        app.canvas_click(Point::new(42., 0.), first_click.clone());
        assert_eq!(
            (app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b),
            (left, center)
        );
        assert_eq!(app.tab.doc.bonds[0].display, preset.parts().1);
        assert_eq!(app.tab.history.undo_frames().count(), 2);
        app.step_history(false);
        assert_eq!(app.tab.doc, first_click);
    }
}

#[test]
fn first_wedge_drag_commit_matches_preview_when_restating_a_plain_bond() {
    use reshiki::bonds::BondPreset as P;
    for preset in [P::Wedge, P::HashedWedge, P::HollowWedge] {
        for reversed_drag in [false, true] {
            let (mut app, _) = App::new();
            let (source, left, center) = backward_chain();
            app.tab.doc = source.clone();
            app.tool = Tool::StyledBond(preset);
            let (a, b) = if reversed_drag {
                (center, left)
            } else {
                (left, center)
            };
            let mut preview = source.clone();
            preset.place(&mut preview, a, b);
            app.place_bond(
                source.atom(a).unwrap().position,
                source.atom(b).unwrap().position,
                Some(a),
                Some(b),
                source,
            );
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.tab.doc.bonds, preview.bonds);
            assert_eq!(
                (app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b),
                (center, left)
            );
        }
    }
}
