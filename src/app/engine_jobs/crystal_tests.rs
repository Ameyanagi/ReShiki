//! A skipped chemistry check must not report molecular success.

use super::*;
use reshiki::engine::Analysis;

fn cached_analysis() -> Analysis {
    // Only UI state is under test; these deliberately empty cached descriptors
    // are never used as an oracle for chemistry.
    Analysis {
        smiles: String::new(),
        formula: String::new(),
        mass: 0.,
        exact_mass: 0.,
        logp: 0.,
        tpsa: 0.,
        donors: 0,
        acceptors: 0,
        rings: 0,
        unpaired_electrons: 0,
        inchi: String::new(),
        inchikey: String::new(),
    }
}

#[test]
fn skipped_check_preserves_drawing_and_does_not_claim_chemical_success() {
    for warnings in [
        vec![],
        vec![reshiki::models3d::crystal::DISTANCE_ANALYSIS_NOTICE.into()],
    ] {
        let (mut app, _) = App::new();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("C", Point::new(28., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.doc.bonds[0].distance_derived = true;
        app.tab
            .doc
            .require_version(reshiki::document::DISTANCE_BONDS);
        app.tab.analysis = Some(cached_analysis());
        app.tab.chemistry_notice = Some("Stale chemistry notice".into());
        app.tab.selected = vec![a];
        app.tab.busy = true;
        let before = app.tab.doc.clone();
        let generation = app.tab.label_generation;
        let revision = app.tab.revision;
        let mut computed = before.clone();
        computed.atoms[0].label_h = 3;
        let expected = if warnings.is_empty() {
            "Chemical analysis unavailable".into()
        } else {
            warnings.join(" · ")
        };
        let _ = app.engine_done(
            revision,
            Job::Analyze,
            Ok(Response {
                document: Some(computed),
                analysis: None,
                output: None,
                engine_version: "test".into(),
                warnings,
            }),
        );
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.label_generation, generation);
        assert_eq!(app.tab.selected, [a]);
        assert!(!app.tab.history.can_undo());
        assert!(app.tab.analysis.is_none() && app.tab.chemistry_notice.is_none());
        assert_eq!(app.status, expected);
        assert!(!app.status.contains("No chemistry errors found"));
        assert!(!app.tab.busy && !app.error);
    }
}

#[test]
fn completed_check_retains_qualified_warnings_and_normal_success_status() {
    for warnings in [vec![], vec!["Qualified chemistry notice".into()]] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.doc.add_atom("O", Point::default());
        let before = app.tab.doc.clone();
        let mut computed = before.clone();
        computed.atoms[0].label_h = 2;
        let _ = app.engine_done(
            app.tab.revision,
            Job::Analyze,
            Ok(Response {
                document: Some(computed),
                analysis: Some(cached_analysis()),
                output: None,
                engine_version: "test".into(),
                warnings: warnings.clone(),
            }),
        );
        assert!(app.tab.analysis.is_some());
        assert_eq!(app.tab.doc.atoms[0].label_h, 2);
        assert!(!app.tab.history.can_undo());
        assert_eq!(
            app.status,
            if warnings.is_empty() {
                "No chemistry errors found"
            } else {
                "Structure checked · Qualified chemistry notice"
            }
        );
        assert!(!app.error);
    }
}
