use super::*;
use crate::document::Point;
fn ethanol() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(42., 0.));
    let c = doc.add_atom("O", Point::new(63., 36.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    doc
}
#[test]
fn complete_component_and_collapsed_groups_retain_drawing_atom_links() {
    let mut doc = ethanol();
    let ids: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    let request = prepare(&doc, &[ids[1]]).unwrap();
    assert_eq!(request.atom_ids, ids);
    assert_eq!(request.source.atoms.len(), 3);
    let another = doc.add_atom("C", Point::new(300., 0.));
    assert!(prepare(&doc, &[]).is_err());
    assert!(prepare(&doc, &[ids[0], another]).is_err());
    assert_eq!(prepare(&doc, &[ids[0]]).unwrap().source.atoms.len(), 3);
}
#[test]
fn geometry_and_paint_preserve_identity_while_chemical_edits_invalidate() {
    let base = ethanol();
    let ids: Vec<_> = base.atoms.iter().map(|a| a.id).collect();
    let identity = fingerprint(&base, &ids).unwrap();
    let mut moved = base.clone();
    for a in &mut moved.atoms {
        a.position.x += 123.;
        a.position.y -= 24.;
        a.depth = 4.;
        a.label_h = 99;
    }
    assert_eq!(identity, fingerprint(&moved, &ids).unwrap());
    moved.atoms[1].element = "N".into();
    assert_ne!(identity, fingerprint(&moved, &ids).unwrap());
    let mut bonded = base.clone();
    bonded.bonds[0].order = 2;
    assert_ne!(identity, fingerprint(&bonded, &ids).unwrap());
    let mut removed = base;
    removed.atoms.pop();
    assert!(fingerprint(&removed, &ids).is_err());
}
#[test]
fn malformed_or_incompatible_index_is_rejected() {
    assert!(parse_index("# encoder=cdk\n1H\t2\ta\t2\t1\t0\t1\t1").is_err());
    assert!(parse_index("# encoder=reshiki-hose-v1\n").is_err());
    let hash = "a".repeat(64);
    assert!(
        parse_index(&format!(
            "# encoder=reshiki-hose-v1\n1H\t2\t{hash}\t2\tNaN\t0\t0\t1"
        ))
        .is_err()
    );
}
#[test]
fn export_identifies_predictions_and_does_not_invent_missing_values() {
    let report = Report {
        nucleus: Nucleus::H1,
        atom_ids: vec![17],
        fingerprint: String::new(),
        rows: vec![Row {
            atom_id: 17,
            hydrogen_ids: vec![],
            hydrogen_count: 2,
            radius: None,
            statistics: None,
            limitation: Some("No compatible data".into()),
        }],
        method: core::METHOD.into(),
        data_version: DATA_VERSION.into(),
        conditions: CONDITIONS.into(),
        limitations: LIMITATIONS.into(),
        attribution: ATTRIBUTION.into(),
    };
    let text = to_tsv(&report);
    assert!(text.starts_with("# PREDICTED"));
    assert!(text.contains("not a calibrated confidence"));
    assert!(text.contains("17\t2\t\t\t\t\t\t\tNo compatible data"));
    assert!(text.contains("nmrshiftdb2 Database License"));
}
#[test]
fn collapsed_abbreviation_has_the_same_full_chemistry_and_identity() {
    let expanded = ethanol();
    let ids: Vec<_> = expanded.atoms.iter().map(|a| a.id).collect();
    let expected = prepare(&expanded, &[ids[0]]).unwrap();
    let mut collapsed = expanded.clone();
    collapsed.contract(&ids[1..], "CH2OH", "HOCH2").unwrap();
    let actual = prepare(&collapsed, &[ids[1]]).unwrap();
    assert_eq!(actual.source.atoms.len(), 3);
    assert_eq!(actual.fingerprint, expected.fingerprint);
    let a = document::prepare(&expected.source).unwrap();
    let b = document::prepare(&actual.source).unwrap();
    assert_eq!(a.state.graph, b.state.graph);
}
#[test]
fn experimentally_assigned_explicit_h_fixture_preserves_original_assignment_indices() {
    let source = include_str!("../../../../tests/fixtures/nmr/explicit-assigned-h.sd");
    let lines: Vec<_> = source.lines().collect();
    let counts = lines
        .iter()
        .position(|line| line.contains("V2000"))
        .unwrap();
    let end = lines.iter().position(|line| *line == "M  END").unwrap();
    let mol = lines[counts - 3..=end].join("\n");
    let imported = crate::chemistry::molfile::read(&mol).unwrap();
    let env = Environment::new(&imported.molecule.state.graph).unwrap();
    let sites = env.sites(Nucleus::H1).unwrap();
    assert_eq!(imported.molecule.state.graph.atoms.len(), 21);
    assert!(
        sites
            .iter()
            .any(|s| s.parent == 5 && s.explicit_hydrogens.contains(&18))
    );
    assert!(
        sites
            .iter()
            .any(|s| s.parent == 8 && s.explicit_hydrogens.contains(&20))
    );
    assert!(sites.iter().any(|s| s.count == 3));
}
#[test]
fn bundled_observed_data_predicts_linked_ethanol_sites_without_exchangeable_guesses() {
    let doc = ethanol();
    let request = prepare(&doc, &[doc.atoms[1].id]).unwrap();
    let report = predict(&request, Nucleus::H1).unwrap();

    assert_eq!(report.rows.len(), 3);
    assert_eq!(
        report
            .rows
            .iter()
            .filter(|r| r.statistics.is_some())
            .count(),
        1
    );
    assert_eq!(
        report
            .rows
            .iter()
            .map(|r| (r.atom_id, r.hydrogen_count))
            .collect::<Vec<_>>(),
        vec![
            (doc.atoms[0].id, 3),
            (doc.atoms[1].id, 2),
            (doc.atoms[2].id, 1)
        ]
    );
    assert!(report.rows[2].statistics.is_none());
    assert!(
        report.rows[2]
            .limitation
            .as_ref()
            .unwrap()
            .contains("Exchangeable")
    );
    assert!(
        report
            .rows
            .iter()
            .filter_map(|r| r.statistics.as_ref())
            .all(|s| s.support >= core::MIN_SUPPORT && s.validate())
    );
    let carbon = predict(&request, Nucleus::C13).unwrap();
    assert_eq!(carbon.rows.len(), 2);
    assert!(carbon.rows.iter().all(|r| r.statistics.is_none()));
    let mut collapsed = doc.clone();
    collapsed
        .contract(&[doc.atoms[1].id, doc.atoms[2].id], "CH2OH", "HOCH2")
        .unwrap();
    let after = predict(
        &prepare(&collapsed, &[doc.atoms[1].id]).unwrap(),
        Nucleus::H1,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&report.rows).unwrap(),
        serde_json::to_value(&after.rows).unwrap()
    );
}
#[test]
fn ethyl_acetate_is_a_fully_supported_small_organic_acceptance_example() {
    let doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/nmr/ethyl-acetate.rsk"
    ))
    .unwrap();
    let request = prepare(&doc, &[1]).unwrap();
    for nucleus in [Nucleus::H1, Nucleus::C13] {
        let report = predict(&request, nucleus).unwrap();
        println!("{}", to_tsv(&report));
        assert!(
            report.rows.iter().all(|r| r.statistics.is_some()),
            "{nucleus:?}"
        );
        assert_eq!(
            report.rows.len(),
            if nucleus == Nucleus::H1 { 3 } else { 4 }
        );
    }
}
