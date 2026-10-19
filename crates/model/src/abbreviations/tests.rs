use super::*;

#[test]
fn abbreviation_containing_returns_the_first_member_match_or_none() {
    let first = Abbreviation {
        label_style: None,
        label_color_override: false,
        highlight: None,
        label: "first".into(),
        reverse_label: String::new(),
        anchor: 1,
        members: vec![1, 2],
        alignment: Default::default(),
    };
    let second = Abbreviation {
        label: "second".into(),
        anchor: 2,
        members: vec![2, 3],
        ..first.clone()
    };
    let doc = Document {
        abbreviations: vec![first.clone(), second.clone()],
        ..Default::default()
    };

    assert_eq!(doc.abbreviation_containing(1), Some(&first));
    assert_eq!(doc.abbreviation_containing(2), Some(&first));
    assert_eq!(doc.abbreviation_containing(3), Some(&second));
    assert_eq!(doc.abbreviation_containing(99), None);
}

#[test]
fn reversed_charged_labels_anchor_on_their_atom() {
    assert_eq!(anchor_range("Me3N+", true), 3..4);
    assert_eq!(anchor_range("F3B-", true), 2..3);
    assert_eq!(anchor_range("NMe3+", false), 0..1);
    assert_eq!(anchor_range("TBSO", true), 3..4);
    // Isothiocyanate and thiocyanate reverse to distinct spellings that end
    // in their attachment atom.
    assert_eq!(anchor_range("S=C=N", true), 4..5);
    let thiocyanate = "N≡CS";
    assert_eq!(&thiocyanate[anchor_range(thiocyanate, true)], "S");
}
