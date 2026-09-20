use moruno::{
    document::{Document, Point},
    pages::{Layout, Margins},
    printing::{self, Scope},
    style::DEFAULT as STYLE,
};

fn drawing() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(-42., 0.));
    let b = doc.add_atom("O", Point::default());
    doc.add_bond(a, b, 2, "plain");
    doc
}
#[test]
fn printing_unpaged_documents_preserves_the_drawing_and_centers_a4_at_publication_scale() {
    let doc = drawing();
    let before = doc.clone();
    let snapshot = printing::snapshot(&doc, &[], Scope::Document).unwrap();
    let layout = snapshot.page_layout.as_ref().unwrap();
    assert_eq!(layout.count(), 1);
    assert!((layout.width_pt * 25.4 / 72. - 210.).abs() < 0.001);
    assert_eq!(snapshot.atoms, doc.atoms);
    assert_eq!(snapshot.bonds, doc.bonds);
    assert_eq!(doc, before);
    assert!(doc.page_layout.is_none());
    let (lo, hi) = moruno::scene::selection_bounds(&snapshot, &snapshot.all_ids()).unwrap();
    let (a, b) = layout.content_bounds(0).unwrap();
    assert!(((lo.x + hi.x) - (a.x + b.x)).abs() < 0.001);
    assert!(((lo.y + hi.y) - (a.y + b.y)).abs() < 0.001);
    assert!(
        (snapshot.atoms[0]
            .position
            .distance(snapshot.atoms[1].position)
            * STYLE.points_per_world()
            - 14.4)
            .abs()
            < 0.001
    );
}
#[test]
fn selection_printing_uses_one_custom_sheet_and_keeps_unselected_content_out() {
    let mut doc = drawing();
    let selected = doc.all_ids();
    doc.add_atom("N", Point::new(3000., 4000.));
    let layout = Layout {
        width_pt: 792.,
        height_pt: 612.,
        columns: 3,
        rows: 2,
        margins: Margins {
            left: 90.,
            right: 18.,
            top: 72.,
            bottom: 36.,
        },
        ..Default::default()
    };
    doc.page_layout = Some(layout.clone());
    let before = doc.clone();
    let snapshot = printing::snapshot(&doc, &selected, Scope::Selection).unwrap();
    let sheet = snapshot.page_layout.as_ref().unwrap();
    assert_eq!(snapshot.all_ids(), selected);
    assert_eq!(snapshot.atoms, drawing().atoms);
    assert_eq!(snapshot.bonds, drawing().bonds);
    assert_eq!(sheet.count(), 1);
    assert_eq!(sheet.width_pt, layout.width_pt);
    assert_eq!(sheet.height_pt, layout.height_pt);
    assert_eq!(sheet.margins, layout.margins);
    let (lo, hi) = moruno::scene::selection_bounds(&snapshot, &snapshot.all_ids()).unwrap();
    let (a, b) = sheet.content_bounds(0).unwrap();
    assert!(((lo.x + hi.x) - (a.x + b.x)).abs() < 0.001);
    assert!(((lo.y + hi.y) - (a.y + b.y)).abs() < 0.001);
    assert_eq!(doc, before);
    assert_eq!(
        printing::snapshot(&doc, &[], Scope::Document)
            .unwrap()
            .page_layout,
        Some(layout)
    );
}
#[test]
fn invalid_or_oversize_prints_fail_without_changing_the_document() {
    assert!(printing::snapshot(&Document::default(), &[], Scope::Document).is_err());
    let mut doc = drawing();
    assert!(printing::snapshot(&doc, &[], Scope::Selection).is_err());
    assert!(printing::snapshot(&doc, &[u64::MAX], Scope::Selection).is_err());
    doc.add_atom("C", Point::new(20_000., 0.));
    let before = doc.clone();
    assert!(printing::snapshot(&doc, &[], Scope::Document).is_err());
    assert!(printing::snapshot(&doc, &doc.all_ids(), Scope::Selection).is_err());
    assert_eq!(doc, before);
}
#[test]
fn preparing_a_print_retains_all_page_dimensions_and_bounds_the_title() {
    let mut doc = drawing();
    doc.page_layout = Some(Layout {
        width_pt: 792.,
        height_pt: 612.,
        columns: 2,
        ..Layout::around(&doc)
    });
    let snapshot = printing::snapshot(&doc, &[], Scope::Document).unwrap();
    let job = printing::prepare(snapshot, format!("\n{}\t", "酸素".repeat(200))).unwrap();
    assert_eq!(job.title.chars().count(), 200);
    assert!(!job.title.contains('\n'));
    let pdf = String::from_utf8_lossy(&job.pdf);
    assert_eq!(pdf.matches("/MediaBox [0 0 792 612]").count(), 2);
    assert!(pdf.contains("/Count 2"));
}

#[test]
fn selected_molecules_keep_hydrogens_and_stereo_labels_after_other_molecules_are_removed() {
    use moruno::atom_labels::{Number, number_style};
    let mut doc = Document::default();
    let a = doc.add_atom("N", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let excluded = doc.add_atom("O", Point::new(80., 20.));
    doc.atom_mut(a).unwrap().label_h = 2;
    doc.atom_mut(b).unwrap().cip_label = Some("R".into());
    doc.atom_labels.stereo = true;
    doc.atom_mut(b).unwrap().display.number = Some(Number {
        text: "42".into(),
        offset: None,
        style: number_style(),
    });
    let original_indicators = moruno::atom_labels::indicators(&doc);
    let snapshot = printing::snapshot(&doc, &[a, b], Scope::Selection).unwrap();
    assert!(snapshot.atom(excluded).is_none());
    assert_eq!(snapshot.atom(a).unwrap().label_h, 2);
    assert_eq!(snapshot.atom(b).unwrap().cip_label.as_deref(), Some("R"));
    let printed_indicators = moruno::atom_labels::indicators(&snapshot);
    assert_eq!(printed_indicators.len(), original_indicators.len());
    for (actual, expected) in printed_indicators.iter().zip(&original_indicators) {
        assert_eq!(actual.text, expected.text);
        assert!(actual.center.distance(expected.center) < 0.001);
    }
}
