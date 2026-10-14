use super::*;

fn labelled_document() -> Document {
    let mut doc = Document::default();
    let first = doc.add_atom("C", Point::new(0., 0.));
    let second = doc.add_atom("O", Point::new(42., 0.));
    let atom = doc.atom_mut(first).unwrap();
    atom.map_num = 27;
    atom.display.mapping.show = Some(true);
    atom.display.mapping.offset = Some(Point::new(0., -38.));
    let atom = doc.atom_mut(second).unwrap();
    atom.map_num = 28;
    atom.display.mapping.show = Some(false);
    atom.display.mapping.offset = Some(Point::new(14., 20.));
    doc
}

#[test]
fn map_range_error_keeps_the_exact_text_and_is_used_by_set_map() {
    let expected = "Enter an atom map from 1 to 2147483647, or 0 to clear";
    assert_eq!(map_range_error(), expected);
    assert_eq!(
        set_map(&labelled_document(), 1, MAX_MAP + 1).unwrap_err(),
        expected,
    );
}

#[test]
fn set_maps_visible_clears_all_overrides_and_preserves_the_other_fields() {
    let source = labelled_document();
    for show in [false, true] {
        let mut expected = source.clone();
        expected.atom_labels.maps = show;
        for atom in &mut expected.atoms {
            atom.display.mapping.show = None;
        }
        assert_eq!(set_maps_visible(&source, show), expected);
    }
    assert_eq!(source, labelled_document());
}

#[test]
fn reset_label_offsets_clears_all_offsets_and_preserves_the_other_fields() {
    let source = labelled_document();
    let mut expected = source.clone();
    for atom in &mut expected.atoms {
        atom.display.mapping.offset = None;
    }
    assert_eq!(reset_label_offsets(&source), expected);
    assert_eq!(source, labelled_document());
}
