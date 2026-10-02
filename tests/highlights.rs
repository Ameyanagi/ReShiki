use reshiki::{
    canvas_theme::{self, CanvasTheme, ColorTheme},
    document::{Document, History, Point},
    editing, highlights,
    palette::{Color, Hue, Palette, Row},
};

fn ether() -> (Document, [u64; 3]) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    let c = doc.add_atom("C", Point::new(63., 36.373));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    (doc, [a, b, c])
}

fn tint(hue: Hue) -> Option<Color> {
    Some(Color::Palette(hue, Row::Tint))
}

#[test]
fn partial_highlights_survive_native_history_copy_and_geometry_without_changing_ink() {
    let (mut doc, [a, b, c]) = ether();
    doc.atom_mut(b).unwrap().text_style = Some(reshiki::typography::TextStyle {
        color: Color::Custom([150, 30, 55]),
        ..Default::default()
    });
    doc.atom_mut(b).unwrap().display.color_override = true;
    doc.bonds[0].color = Color::Custom([30, 60, 90]);
    let original = doc.clone();
    assert_eq!(highlights::apply(&mut doc, &[a, b], tint(Hue::Amber)), 3);
    assert_eq!(doc.atom(a).unwrap().display.highlight, tint(Hue::Amber));
    assert_eq!(doc.atom(b).unwrap().display.highlight, tint(Hue::Amber));
    assert_eq!(doc.atom(c).unwrap().display.highlight, None);
    assert_eq!(doc.bonds[0].highlight, tint(Hue::Amber));
    assert_eq!(doc.bonds[1].highlight, None);
    let painted = doc.clone();
    let mut history = History::default();
    assert!(history.commit(original.clone(), &painted));
    highlights::apply(&mut doc, &[a, b], tint(Hue::Amber));
    assert!(!history.commit(painted.clone(), &doc));
    assert!(history.undo(&mut doc));
    assert_eq!(doc, original);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, painted);
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened, painted.current());
    assert_eq!(reopened.version, reshiki::document::VERSION);
    assert_eq!(
        reopened.atom(b).unwrap().text_style,
        original.atom(b).unwrap().text_style
    );
    assert_eq!(reopened.bonds[0].color, original.bonds[0].color);
    let part = editing::selection(&doc, &[a, b]);
    assert_eq!(part.atoms.len(), 2);
    assert_eq!(part.bonds.len(), 1);
    let mut pasted = original.clone();
    let added = editing::append(&mut pasted, &part, Point::new(200., 100.));
    assert!(added.iter().all(|id| ![a, b, c].contains(id)));
    assert_eq!(
        highlights::selected_colors(&pasted, &added),
        vec![tint(Hue::Amber); 3]
    );
    let first = pasted.atom(added[0]).unwrap().position;
    pasted.translate(&added, 10., -20.);
    editing::scale_axes_about(&mut pasted, &added, first, 1.3, 0.7);
    assert_eq!(
        highlights::selected_colors(&pasted, &added),
        vec![tint(Hue::Amber); 3]
    );
    pasted.delete(&added);
    assert!(!highlights::any(&pasted));
    highlights::apply(&mut doc, &[a, b], None);
    assert_eq!(doc, original, "only highlight appearance changed");
    let old = serde_json::to_vec(&original).unwrap();
    assert!(!String::from_utf8_lossy(&old).contains("highlight"));
    assert_eq!(Document::from_json(&old).unwrap(), original);
}

#[test]
fn contracted_labels_keep_existing_internal_colors_and_propagate_only_to_unpainted_members() {
    let (mut doc, [a, b, c]) = ether();
    doc.atom_mut(b).unwrap().display.highlight = tint(Hue::Red);
    doc.bonds[1].highlight = tint(Hue::Blue);
    doc.contract(&[b, c], "OMe", "MeO").unwrap();
    assert_eq!(highlights::apply(&mut doc, &[b], tint(Hue::Teal)), 1);
    assert_eq!(doc.abbreviation(b).unwrap().highlight, tint(Hue::Teal));
    assert_eq!(
        highlights::selected_colors(&doc, &[b]),
        vec![tint(Hue::Teal)]
    );
    assert_eq!(doc.atom(a).unwrap().display.highlight, None);
    assert_eq!(doc.atom(b).unwrap().display.highlight, tint(Hue::Red));
    assert_eq!(doc.atom(c).unwrap().display.highlight, tint(Hue::Teal));
    assert_eq!(doc.bonds[1].highlight, tint(Hue::Blue));
    let part = editing::selection(&doc, &[b]);
    assert_eq!(part.abbreviations[0].highlight, tint(Hue::Teal));
    let mut expanded = doc.clone();
    expanded.expand_abbreviations(&[b]);
    assert_eq!(expanded.atoms, doc.atoms);
    assert_eq!(expanded.bonds, doc.bonds);
    highlights::apply(&mut doc, &[b], tint(Hue::Purple));
    assert_eq!(doc.abbreviation(b).unwrap().highlight, tint(Hue::Purple));
    assert_eq!(doc.atom(b).unwrap().display.highlight, tint(Hue::Red));
    assert_eq!(doc.atom(c).unwrap().display.highlight, tint(Hue::Teal));
    highlights::apply(&mut doc, &[b], None);
    assert!(!highlights::any(&doc));
}

#[test]
fn theme_resolution_and_paste_preserve_exact_highlight_colors_once() {
    for canvas in CanvasTheme::ALL {
        for theme in ColorTheme::ALL {
            let (mut doc, [_, b, c]) = ether();
            doc.canvas_theme = canvas;
            theme.apply(&mut doc);
            doc.contract(&[b, c], "OMe", "MeO").unwrap();
            highlights::apply(&mut doc, &[b], tint(Hue::Blue));
            doc.atom_mut(c).unwrap().display.highlight = Some(Color::Custom([0, 0, 0]));
            let palette = Palette::of(&doc);
            let visible = palette.rgb(tint(Hue::Blue).unwrap());
            let before = doc.clone();
            let resolved = canvas_theme::resolved_document(&doc).into_owned();
            assert_eq!(doc, before, "resolution is read-only");
            assert_eq!(
                resolved.abbreviation(b).unwrap().highlight,
                Some(Color::Custom(visible))
            );
            assert_eq!(
                resolved.atom(c).unwrap().display.highlight,
                Some(Color::Custom([0; 3]))
            );
            assert_eq!(
                canvas_theme::resolved_document(&resolved).as_ref(),
                &resolved
            );
            for target in CanvasTheme::ALL {
                let native = canvas_theme::for_native_paste(doc.clone(), target);
                assert_eq!(native.abbreviation(b).unwrap().highlight, tint(Hue::Blue));
                assert_eq!(
                    native.atom(c).unwrap().display.highlight,
                    Some(Color::Custom([0; 3]))
                );
                let pasted = canvas_theme::for_paste(doc.clone(), target);
                assert_eq!(
                    pasted.abbreviation(b).unwrap().highlight,
                    Some(Color::Custom(visible))
                );
                assert_eq!(
                    pasted.atom(c).unwrap().display.highlight,
                    Some(Color::Custom([0; 3]))
                );
            }
            ColorTheme::Publication.apply(&mut doc);
            assert_eq!(
                doc.abbreviation(b).unwrap().highlight,
                before.abbreviation(b).unwrap().highlight
            );
            assert_eq!(
                doc.atom(c).unwrap().display.highlight,
                before.atom(c).unwrap().display.highlight
            );
        }
    }
}

#[test]
fn changing_bond_order_keeps_highlight_and_atom_only_highlights_do_not_paint_incident_bonds() {
    let (mut doc, [a, b, _]) = ether();
    assert_eq!(highlights::apply(&mut doc, &[b], tint(Hue::Purple)), 1);
    assert!(doc.bonds.iter().all(|bond| bond.highlight.is_none()));
    highlights::apply(&mut doc, &[a, b], tint(Hue::Purple));
    doc.add_bond(a, b, 2, "plain");
    assert_eq!(doc.bonds[0].highlight, tint(Hue::Purple));
    assert_eq!(doc.bonds[0].order, 2);
}

#[test]
fn shared_atoms_and_fused_bonds_keep_paint_when_duplicate_geometry_is_removed() {
    use reshiki::templates::{self, Anchor, Connection};
    for destination_color in [None, tint(Hue::Red)] {
        let mut destination = Document::default();
        let a = destination.add_atom("C", Point::default());
        let b = destination.add_atom("C", Point::new(42., 0.));
        destination.add_bond(a, b, 1, "plain");
        highlights::apply(&mut destination, &[a, b], destination_color);
        let mut source = Document::default();
        let x = source.add_atom("C", Point::default());
        let y = source.add_atom("C", Point::new(42., 0.));
        source.add_bond(x, y, 1, "plain");
        highlights::apply(&mut source, &[x, y], tint(Hue::Blue));
        let (shared, _) = templates::place_with_mode(
            &destination,
            &source,
            Point::new(42., 0.),
            Some(Point::new(84., 0.)),
            5.,
            Anchor::Atom(x),
            Connection::ShareAtom,
        )
        .unwrap();
        assert_eq!(
            shared.atom(b).unwrap().display.highlight,
            destination_color.or(tint(Hue::Blue))
        );
        let (fused, _) = templates::place_with_mode(
            &destination,
            &source,
            Point::new(21., 0.),
            Some(Point::new(21., 42.)),
            5.,
            Anchor::Bond(x, y),
            Connection::FuseBond,
        )
        .unwrap();
        assert_eq!(fused.bonds.len(), 1);
        assert_eq!(
            fused.bonds[0].highlight,
            destination_color.or(tint(Hue::Blue))
        );
        assert!(
            fused
                .atoms
                .iter()
                .all(|atom| atom.display.highlight == destination_color.or(tint(Hue::Blue)))
        );
    }
}

#[test]
fn typed_preset_replacement_keeps_label_and_anchor_paint_and_colors_new_members() {
    use reshiki::atom_text::{self, Mode};
    for (element, first, replacement) in [("O", "OMe", "OEt"), ("C", "Cp", "Cp*")] {
        let mut doc = Document::default();
        let anchor = doc.add_atom(element, Point::default());
        highlights::apply(&mut doc, &[anchor], tint(Hue::Red));
        doc = atom_text::apply(&doc, anchor, first, Mode::Group).unwrap();
        assert_eq!(doc.abbreviation(anchor).unwrap().highlight, tint(Hue::Red));
        highlights::apply(&mut doc, &[anchor], tint(Hue::Blue));
        assert_eq!(doc.atom(anchor).unwrap().display.highlight, tint(Hue::Red));
        let replaced = atom_text::apply(&doc, anchor, replacement, Mode::Group).unwrap();
        assert_eq!(
            replaced.abbreviation(anchor).unwrap().highlight,
            tint(Hue::Blue)
        );
        assert_eq!(
            replaced.atom(anchor).unwrap().display.highlight,
            tint(Hue::Red)
        );
        assert!(
            replaced
                .atoms
                .iter()
                .filter(|a| a.id != anchor)
                .all(|a| a.display.highlight == tint(Hue::Blue))
        );
        assert!(
            replaced
                .bonds
                .iter()
                .all(|b| b.highlight == tint(Hue::Blue))
        );
        replaced.validate().unwrap();
    }
}
