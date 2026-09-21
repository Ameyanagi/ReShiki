use reshiki::{
    document::{Document, Point},
    pages::{Layout, Preset},
    style::DEFAULT as STYLE,
};

fn drawing() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(-42., 0.));
    let b = doc.add_atom("C", Point::default());
    doc.add_bond(a, b, 1, "plain");
    doc
}
#[test]
fn legacy_documents_remain_unbounded_and_layout_roundtrips() {
    let legacy: Document =
        serde_json::from_str(include_str!("fixtures/ui-drawn-ethanol.reshiki")).unwrap();
    assert!(legacy.page_layout.is_none());
    let mut doc = drawing();
    let before = doc.clone();
    doc.page_layout = Some(Layout::around(&doc));
    let loaded: Document = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
    loaded.validate().unwrap();
    assert_eq!(loaded, doc);
    assert_eq!(doc.atoms, before.atoms);
    assert_eq!(doc.bonds, before.bonds);
}
#[test]
fn paper_sizes_and_grid_use_physical_points_and_reading_order() {
    let mut layout = Layout {
        columns: 2,
        rows: 2,
        ..Default::default()
    };
    let a = layout.bounds(0).unwrap();
    let b = layout.bounds(1).unwrap();
    let c = layout.bounds(2).unwrap();
    assert!(((a.1.x - a.0.x) * STYLE.points_per_world() * 25.4 / 72. - 210.).abs() < 0.001);
    assert!(((a.1.y - a.0.y) * STYLE.points_per_world() * 25.4 / 72. - 297.).abs() < 0.001);
    assert_eq!(b.0.y, a.0.y);
    assert_eq!(c.0.x, a.0.x);
    assert!((b.0.x - a.1.x - STYLE.world(18.)).abs() < 0.001);
    assert_eq!(
        layout.spread_bounds().unwrap().1,
        layout.bounds(3).unwrap().1
    );
    assert!(layout.bounds(4).is_none());
    let (w, h) = Preset::Letter.size().unwrap();
    layout.width_pt = h;
    layout.height_pt = w;
    assert_eq!(layout.preset(), Preset::Letter);
}
#[test]
fn centering_respects_asymmetric_margins_and_keeps_whole_molecule_geometry() {
    let mut doc = drawing();
    let before = doc.clone();
    let mut layout = Layout {
        columns: 2,
        ..Default::default()
    };
    layout.margins.left = 72.;
    layout.center(&mut doc, &[1], 1).unwrap();
    let (lo, hi) = reshiki::scene::selection_bounds(&doc, &[1, 2]).unwrap();
    let (a, b) = layout.content_bounds(1).unwrap();
    assert!(((lo.x + hi.x) - (a.x + b.x)).abs() < 0.001);
    assert!(((lo.y + hi.y) - (a.y + b.y)).abs() < 0.001);
    assert!((doc.atoms[0].position.distance(doc.atoms[1].position) - 42.).abs() < 0.001);
    assert_eq!(doc.bonds, before.bonds);
    assert_eq!(layout.overflow(&doc), 0);
    doc.translate(&doc.all_ids(), STYLE.world(layout.width_pt), 0.);
    assert!(layout.overflow(&doc) > 0);
}
#[test]
fn malformed_page_layouts_fail_without_mutating_drawing() {
    let doc = drawing();
    for layout in [
        Layout {
            columns: 0,
            ..Default::default()
        },
        Layout {
            rows: 11,
            ..Default::default()
        },
        Layout {
            width_pt: f32::NAN,
            ..Default::default()
        },
        Layout {
            height_pt: 0.,
            ..Default::default()
        },
        Layout {
            origin: Point::new(f32::INFINITY, 0.),
            ..Default::default()
        },
        Layout {
            margins: reshiki::pages::Margins {
                top: 1000.,
                ..Default::default()
            },
            ..Default::default()
        },
    ] {
        let mut copy = doc.clone();
        assert!(layout.center(&mut copy, &[1], 0).is_err());
        assert_eq!(copy, doc);
        copy.page_layout = Some(layout);
        assert!(copy.validate().is_err());
        assert!(reshiki::export::pages_pdf(&copy).is_err());
    }
}
#[test]
fn page_pdf_uses_all_sheets_while_drawing_exports_stay_cropped() {
    let mut doc = drawing();
    let cropped = reshiki::scene::svg(&doc);
    assert!(reshiki::export::pages_pdf(&doc).is_err());
    doc.page_layout = Some(Layout {
        width_pt: 612.,
        height_pt: 792.,
        columns: 2,
        rows: 2,
        ..Default::default()
    });
    assert_eq!(reshiki::scene::svg(&doc), cropped);
    let pdf = reshiki::export::pages_pdf(&doc).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.starts_with("%PDF-"));
    assert!(text.contains("/Count 4"));
    assert_eq!(text.matches("/MediaBox [0 0 612 792]").count(), 4);
    assert_eq!(text.matches("/Type /Page\n").count(), 4);
}
#[tokio::test]
async fn chemistry_analysis_and_selection_cleanup_preserve_page_metadata() {
    use reshiki::engine::{LocalEngine, Request};
    let engine = LocalEngine::default();
    let mut doc = engine
        .request(Request::import_smiles("CCO"))
        .await
        .unwrap()
        .document
        .unwrap();
    let mut layout = Layout::around(&doc);
    layout.columns = 2;
    doc.page_layout = Some(layout);
    for operation in ["analyze", "clean"] {
        let mut request = Request::molecule(operation, doc.clone());
        if operation == "clean" {
            request.selected_ids = Some(doc.all_ids());
            request.cleanup = Some(Default::default());
        }
        let checked = engine.request(request).await.unwrap().document.unwrap();
        assert_eq!(checked.page_layout, doc.page_layout);
        checked.validate().unwrap();
    }
}

#[test]
fn a_bond_spanning_two_pages_is_reported_even_when_its_atoms_fit() {
    let layout = Layout {
        columns: 2,
        ..Default::default()
    };
    let mut doc = Document::default();
    let (a, b) = layout.content_bounds(0).unwrap();
    let (c, d) = layout.content_bounds(1).unwrap();
    let left = doc.add_atom("C", Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.));
    let right = doc.add_atom("C", Point::new((c.x + d.x) / 2., (c.y + d.y) / 2.));
    doc.add_bond(left, right, 1, "plain");
    assert!(layout.overflow(&doc) > 0);
}
