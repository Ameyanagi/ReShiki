use super::*;
use reshiki::document::Annotation;

#[test]
fn selected_wrapper_typography_and_color_leave_internal_atom_and_bond_styles_untouched() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let anchor = app.tab.doc.add_atom("O", Point::default());
    let member = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(anchor, member, 1, "plain");
    app.tab.doc.atom_mut(anchor).unwrap().text_style = Some(TextStyle {
        size_pt: 11.,
        color: Paint::Custom([0; 3]),
        ..Default::default()
    });
    app.tab.doc.atom_mut(anchor).unwrap().display.color_override = true;
    app.tab.doc.bonds[0].color = Paint::Custom([30, 70, 110]);
    app.tab
        .doc
        .contract(&[anchor, member], "OMe", "MeO")
        .unwrap();
    app.tab.doc.reconcile_molecule_groups();
    app.tab.selected = app.tab.doc.expand_abbreviation_selection(&[anchor]);
    app.tab.labels_dirty = false;
    app.sync_typography();
    let original = app.tab.doc.clone();
    assert_eq!(app.current_text_style().size_pt, 11.);
    let _ = app.update(Message::TextStyle(StyleChange::Size(18.)));
    let _ = app.update(Message::TextStyle(StyleChange::Bold(true)));
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    let group = app.tab.doc.abbreviation(anchor).unwrap();
    assert_eq!(group.text_style(&app.tab.doc).size_pt, 18.);
    assert!(group.text_style(&app.tab.doc).bold);
    assert!(
        group.label_color_override,
        "font edits preserve legacy explicit ink"
    );
    let before_color = app.tab.doc.clone();
    let color = Paint::Custom([124; 3]);
    let _ = app.update(Message::ColorScope(ColorScope::All));
    let _ = app.update(Message::TextStyle(StyleChange::Color(color)));
    assert_eq!(app.current_selection_color(), Some(color));
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    assert!(!chemistry_changed(&original, &app.tab.doc));
    assert!(!app.tab.labels_dirty);
    let painted = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before_color);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, painted);
    app.sync_typography();
    assert_eq!(app.current_text_style().size_pt, 18.);
    assert_eq!(app.current_selection_color(), Some(color));
    app.tab.doc.expand_abbreviations(&[anchor]);
    assert_eq!(app.tab.doc.atoms, original.atoms);
}

#[test]
fn wrapper_font_edits_keep_automatic_ink_until_the_color_is_explicitly_chosen() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let anchor = group(&mut app, "OMe", 0.).unwrap();
    app.tab.doc.reconcile_molecule_groups();
    app.tab.selected = vec![anchor];
    app.sync_typography();
    let internal = app.tab.doc.atoms.clone();
    let _ = app.update(Message::TextStyle(StyleChange::Size(16.)));
    assert!(
        !app.tab
            .doc
            .abbreviation(anchor)
            .unwrap()
            .label_color_override
    );
    let _ = app.update(Message::TextStyle(StyleChange::Color(Paint::Ink)));
    assert!(
        app.tab
            .doc
            .abbreviation(anchor)
            .unwrap()
            .label_color_override
    );
    assert_eq!(app.tab.doc.atoms, internal);
}

#[test]
fn highlight_scope_applies_and_clears_without_changing_ink_or_chemistry() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.reconcile_molecule_groups();
    app.tab.selected = vec![a, b];
    app.tab.labels_dirty = false;
    let original = app.tab.doc.clone();
    let color = Paint::Palette(reshiki::palette::Hue::Amber, reshiki::palette::Row::Tint);
    let _ = app.update(Message::ColorScope(ColorScope::Highlights));
    let _ = app.update(Message::TextStyle(StyleChange::Color(color)));
    assert!(reshiki::highlights::any(&app.tab.doc));
    assert!(!chemistry_changed(&original, &app.tab.doc));
    assert!(
        !app.tab.labels_dirty,
        "appearance keeps calculated labels current"
    );
    assert_eq!(app.current_selection_color(), Some(color));
    let painted = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::TextStyle(StyleChange::Color(color)));
    assert_eq!(app.tab.revision, revision, "same paint is a no-op");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, painted);
    let _ = app.update(Message::ClearHighlights);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, painted);
    assert!(super::super::color_popover::keeps_open(
        &Message::ClearHighlights
    ));
}

fn group(app: &mut App, label: &str, x: f32) -> Result<u64, String> {
    let id = app.tab.doc.add_atom("C", Point::new(x, 0.));
    app.tab.doc =
        reshiki::atom_text::apply(&app.tab.doc, id, label, reshiki::atom_text::Mode::Group)?;
    Ok(id)
}

#[test]
fn toolbar_aligns_selected_groups_and_captions_in_one_undo_without_changing_chemistry()
-> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let boc = group(&mut app, "Boc", 0.)?;
    let cp = group(&mut app, "Cp*", 150.)?;
    let other = group(&mut app, "OMe", 300.)?;
    let caption = app.tab.doc.next_id();
    app.tab.doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(0., 200.),
        text: "Caption".into(),
        format: Default::default(),
    });
    app.tab.selected = vec![boc];
    assert_eq!(
        app.selected_group_alignment(),
        Some(Some(LabelAlignment::Auto))
    );
    assert_eq!(
        app.toolbar_alignment(),
        None,
        "Automatic must not highlight Left"
    );
    let _ = app.update(Message::TextAlign(TextAlign::Right));
    assert_eq!(app.toolbar_alignment(), Some(TextAlign::Right));
    app.tab.selected = vec![boc, cp, caption];
    assert_eq!(app.selected_group_alignment(), Some(None));
    assert_eq!(app.toolbar_alignment(), None);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::TextAlign(TextAlign::Center));
    assert_eq!(app.toolbar_alignment(), Some(TextAlign::Center));
    for id in [boc, cp] {
        assert_eq!(
            app.tab.doc.abbreviation(id).ok_or("group")?.alignment,
            LabelAlignment::Center
        );
    }
    assert_eq!(
        app.tab
            .doc
            .abbreviation(other)
            .ok_or("other group")?
            .alignment,
        LabelAlignment::Auto
    );
    assert_eq!(app.tab.doc.atoms, before.atoms);
    assert_eq!(app.tab.doc.bonds, before.bonds);
    let after = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);

    let _ = app.update(Message::TextAlign(TextAlign::Justified));
    assert_eq!(
        app.tab.doc.abbreviation(boc).ok_or("group")?.alignment,
        LabelAlignment::Center
    );
    assert_eq!(
        app.tab
            .doc
            .annotations
            .first()
            .ok_or("caption")?
            .format
            .alignment,
        TextAlign::Justified
    );
    assert_eq!(
        app.toolbar_alignment(),
        None,
        "Mixed caption/group alignment"
    );
    let _ = app.update(Message::GroupLabelAlign(LabelAlignment::Auto));
    assert_eq!(
        app.selected_group_alignment(),
        Some(Some(LabelAlignment::Auto))
    );
    assert_eq!(
        app.tab
            .doc
            .annotations
            .first()
            .ok_or("caption")?
            .format
            .alignment,
        TextAlign::Justified
    );
    let _ = app.update(Message::GroupLabelAlign(LabelAlignment::Above));
    assert_eq!(
        app.selected_group_alignment(),
        Some(Some(LabelAlignment::Above))
    );
    assert_eq!(app.tab.doc.atoms, before.atoms);
    assert_eq!(app.tab.doc.bonds, before.bonds);
    Ok(())
}

#[test]
fn group_only_alignment_does_not_change_paragraph_defaults_or_unselected_groups()
-> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let id = group(&mut app, "Boc", 0.)?;
    app.tab.selected = app.tab.doc.abbreviation(id).ok_or("group")?.members.clone();
    let format = app.tab.caption_format.clone();
    for alignment in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
        let _ = app.update(Message::TextAlign(alignment));
        assert_eq!(app.toolbar_alignment(), Some(alignment));
        assert_eq!(app.tab.caption_format, format);
    }
    let before = app.tab.doc.clone();
    let _ = app.update(Message::TextAlign(TextAlign::Justified));
    assert_eq!(app.tab.doc, before);
    let second = group(&mut app, "Cp", 200.)?;
    assert_eq!(
        app.tab
            .doc
            .abbreviation(second)
            .ok_or("new group")?
            .alignment,
        LabelAlignment::Auto
    );
    app.tab.selected.clear();
    let _ = app.update(Message::TextAlign(TextAlign::Justified));
    assert_eq!(app.tab.caption_format.alignment, TextAlign::Justified);
    assert_eq!(
        app.tab.doc.abbreviation(id).ok_or("group")?.alignment,
        LabelAlignment::Right
    );
    Ok(())
}

#[test]
fn inline_caption_alignment_never_moves_group_labels_and_can_be_cancelled() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let id = group(&mut app, "Boc", 0.)?;
    let caption = app.tab.doc.next_id();
    app.tab.doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(0., 100.),
        text: "Caption".into(),
        format: Default::default(),
    });
    let before = app.tab.doc.clone();
    let _ = app.update(Message::InlineText(
        super::super::inline_text::Action::Begin(Some(caption), Point::default()),
    ));
    app.tab.selected = vec![id, caption];
    assert_eq!(app.selected_group_alignment(), None);
    let _ = app.update(Message::GroupLabelAlign(LabelAlignment::Above));
    let _ = app.update(Message::TextAlign(TextAlign::Right));
    assert_eq!(app.toolbar_alignment(), Some(TextAlign::Right));
    assert_eq!(
        app.tab.doc, before,
        "Inline formatting only changes the draft"
    );
    let _ = app.update(Message::InlineText(
        super::super::inline_text::Action::Finish(false),
    ));
    assert_eq!(app.tab.doc, before);
    Ok(())
}
