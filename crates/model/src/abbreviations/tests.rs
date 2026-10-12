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
