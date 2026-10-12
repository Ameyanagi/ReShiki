use super::super::test_support::{accept, ethyl_acetate_doc, predicted};
use super::*;

fn two_sites(nucleus: Nucleus) -> Arc<Report> {
    let document = ethyl_acetate_doc();
    let mut report = (*predicted(&document, nucleus)).clone();
    let mut supported = report.rows[0].clone();
    supported.atom_id = 9;
    supported.hydrogen_count = 3;
    supported.radius = Some(2);
    let statistics = supported.statistics.as_mut().unwrap();
    statistics.median = 1.25;
    statistics.support = 7;
    statistics.standard_deviation = 0.125;
    let mut unsupported = supported.clone();
    unsupported.atom_id = 4;
    unsupported.hydrogen_count = 1;
    unsupported.radius = None;
    unsupported.statistics = None;
    unsupported.limitation = Some("No matching data".into());
    report.rows = vec![supported, unsupported];
    report.data_version = "local-data-v1".into();
    report.conditions = "CDCl₃; 298 K".into();
    Arc::new(report)
}

#[test]
fn row_presentation_preserves_proton_and_carbon_text_bytes() {
    for (nucleus, supported_label, unsupported_label, description, unsupported) in [
        (
            Nucleus::H1,
            "#9 · 3 H group",
            "#4 · 1 H",
            concat!(
                "Select ¹H NMR site atom #9: 3 attached H in an unresolved group. ",
                "Predicted 1.250 ppm; sphere 2; 7 independent references; observed SD 0.125 ppm",
            ),
            "Select ¹H NMR site atom #4: 1 attached H. Unsupported: No matching data",
        ),
        (
            Nucleus::C13,
            "C #9",
            "C #4",
            concat!(
                "Select ¹³C NMR site atom #9: carbon atom. ",
                "Predicted 1.250 ppm; sphere 2; 7 independent references; observed SD 0.125 ppm",
            ),
            "Select ¹³C NMR site atom #4: carbon atom. Unsupported: No matching data",
        ),
    ] {
        let report = two_sites(nucleus);
        let presentation = Presentation::new(&report);
        assert_eq!(presentation.rows[0].label, supported_label);
        assert_eq!(presentation.rows[1].label, unsupported_label);
        assert_eq!(presentation.rows[0].description, description);
        assert_eq!(presentation.rows[1].description, unsupported);
        assert_eq!(presentation.rows[0].shift, "1.250");
        assert_eq!(presentation.rows[1].shift, "—");
        assert_eq!(
            presentation.rows[0].details,
            "Sphere 2 · Refs 7 · Obs. SD 0.125 ppm",
        );
        assert_eq!(
            presentation.rows[1].details,
            "Sphere — · Refs — · Obs. SD — ppm",
        );
        assert_eq!(presentation.value, "local-data-v1. CDCl₃; 298 K");
        assert_eq!(presentation.supported_count, 1);
        assert_eq!(presentation.sorted_ids, [9]);
    }
}

#[test]
fn presentation_distinguishes_supported_rows_from_finite_unique_owners() {
    let source = two_sites(Nucleus::C13);
    let mut report = (*source).clone();
    let supported = report.rows[0].clone();
    report.rows = [30, 7, 30, 2, 50]
        .into_iter()
        .map(|atom_id| {
            let mut row = supported.clone();
            row.atom_id = atom_id;
            row
        })
        .collect();
    report.rows[1].statistics.as_mut().unwrap().median = f64::NAN;
    report.rows[3].statistics = None;
    let report = Arc::new(report);
    let presentation = Presentation::new(&report);
    assert_eq!(presentation.supported_count, 4);
    assert_eq!(presentation.sorted_ids, [30, 50]);
    assert_eq!(presentation.rows[1].shift, "NaN");
    assert!(
        presentation.rows[1]
            .description
            .contains("Predicted NaN ppm")
    );
}

#[test]
fn direct_result_assignment_uses_fallback_instead_of_another_arcs_cache() {
    let mut app = crate::app::App::new().0;
    app.tab.doc = ethyl_acetate_doc();
    let report = predicted(&app.tab.doc, Nucleus::C13);
    accept(&mut app, Arc::clone(&report));
    app.tab.nmr.labels.visible = true;
    let presentation = app.tab.nmr.job.presentation.as_ref().unwrap();
    assert!(presentation.matches(&Arc::clone(&report)));
    assert_eq!(
        app.nmr_canvas().unwrap().supported,
        Some(presentation.sorted_ids.as_slice()),
    );
    let before = super::super::view::panel_description(&app.tab.nmr);
    app.tab.nmr.job.result = Some(Arc::new((*report).clone()));
    assert!(app.nmr_canvas().unwrap().supported.is_none());
    assert_eq!(super::super::view::panel_description(&app.tab.nmr), before);

    let mut replacement = (*report).clone();
    for row in &mut replacement.rows {
        row.statistics = None;
    }
    app.tab.nmr.job.result = Some(Arc::new(replacement));
    assert!(
        super::super::view::panel_description(&app.tab.nmr)
            .contains("¹³C prediction: 0 of 4 atom-linked sites supported"),
    );
    assert!(app.nmr_canvas().unwrap().supported.is_none());
}
