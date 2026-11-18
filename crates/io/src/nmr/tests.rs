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
/// A drawing of `smiles` with atom IDs 1..=n in SMILES order, as import
/// numbers them.
fn from_smiles(smiles: &str) -> Document {
    let prepared = crate::chemistry::smiles::prepare(smiles).unwrap();
    let count = prepared.state.graph.atoms.len();
    let molecule = document::Molecule {
        rdkit_version: crate::chemistry::RDKIT_VERSION,
        ids: (1..=count as u64).collect(),
        positions: vec![crate::chemistry::stereo::Point3::default(); count],
        state: prepared.state,
    };
    let drawing = document::for_import(&molecule, false, &prepared.dummy_labels).unwrap();
    let labels = drawing.labels().unwrap();
    drawing.finish(labels).unwrap()
}
fn ethyl_acetate() -> Document {
    Document::from_json(include_bytes!(
        "../../../../tests/fixtures/nmr/ethyl-acetate.rsk"
    ))
    .unwrap()
}
#[test]
fn complete_component_and_collapsed_groups_retain_drawing_atom_links() {
    let mut doc = ethanol();
    let ids: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    let request = prepare(&doc, &[ids[1]]).unwrap();
    assert_eq!(request.atom_ids, ids);
    assert_eq!(request.source.atoms.len(), 3);
    let another = doc.add_atom("C", Point::new(300., 0.));
    assert_eq!(prepare(&doc, &[]).unwrap_err(), SELECT_ONE_MOLECULE);
    assert_eq!(
        prepare(&doc, &[ids[0], another]).unwrap_err(),
        SELECT_ONE_MOLECULE
    );
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
            broad: false,
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
        assert!(report.rows.iter().all(|row| row.limitation.is_none()));
    }
}

#[test]
fn acetyl_arylamide_methyls_retain_their_shift_with_an_explicit_broad_match_note() {
    for (smiles, supported) in [("CC(=O)Nc1ccc(O)cc1", 5), ("CC(=O)Nc1ccccc1", 6)] {
        let doc = from_smiles(smiles);
        let report = predict(&prepare(&doc, &[]).unwrap(), Nucleus::H1).unwrap();
        let (index, methyl) = report
            .rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.hydrogen_count == 3)
            .unwrap();
        let statistics = methyl.statistics.as_ref().unwrap();
        assert_eq!(methyl.atom_id, 1, "{smiles}");
        assert_eq!(methyl.radius, Some(2), "{smiles}");
        assert!((statistics.median - 1.500625).abs() < 1e-6, "{smiles}");
        assert_eq!(statistics.support, 156, "{smiles}");
        assert!((statistics.standard_deviation - 0.5074984).abs() < 1e-6);
        assert_eq!(
            methyl.limitation.as_deref(),
            Some("Broad match · Shell 2 · SD 0.507 ppm"),
            "{smiles}"
        );
        assert_eq!(report.rows.len(), 7, "{smiles}");
        assert_eq!(report.supported_count(), supported, "{smiles}");
        assert_eq!(report.shifts().count(), supported, "{smiles}");
        assert!(
            report
                .shifts()
                .any(|shift| shift == (index, statistics.median))
        );
        assert!(to_tsv(&report).contains(
            "1\t3\t1.501\t2\t156\t0.507\t0.580\t2.730\tBroad match · Shell 2 · SD 0.507 ppm"
        ));
    }
}

#[test]
fn generic_nmr_gate_covers_connected_dependencies_but_not_independent_components() {
    use reshiki_model::generic::{self, Crossing, RepeatCount, RepeatPattern, RepeatUnit};
    let plain = ethanol();
    let identity = fingerprint(&plain, &[1, 2, 3]).unwrap();
    let unit = |inside, left, right| RepeatUnit {
        bracket: None,
        bracket_objects: vec![],
        members: vec![inside],
        crossings: vec![
            Crossing {
                inside,
                outside: left,
            },
            Crossing {
                inside,
                outside: right,
            },
        ],
        crossing_brackets: vec![],
        head_count: Some(1),
        usage: "SRU".into(),
        pattern: RepeatPattern::HeadToTail,
        count: RepeatCount::Finite { min: 1, max: 3 },
        label: "1-3".into(),
        flip: "NoFlip".into(),
        label_offset: None,
    };
    let mut touched = plain.clone();
    generic::set_repeat_unit(&mut touched, unit(2, 1, 3)).unwrap();
    let before = touched.clone();
    assert!(
        fingerprint(&touched, &[1])
            .unwrap_err()
            .contains("Expand generic structures")
    );
    assert!(fingerprint(&touched, &[1, 2, 3]).is_err());
    assert!(
        prepare(&touched, &[1])
            .unwrap_err()
            .contains("Expand generic structures")
    );
    assert_eq!(touched, before);

    let mut independent = plain.clone();
    let ids = editing::append(&mut independent, &plain, Point::new(300., 0.));
    generic::set_repeat_unit(&mut independent, unit(ids[1], ids[0], ids[2])).unwrap();
    assert_eq!(fingerprint(&independent, &[1, 2, 3]).unwrap(), identity);
    let request = prepare(&independent, &[1]).unwrap();
    assert_eq!(request.atom_ids, vec![1, 2, 3]);
    assert!(request.source.generic.is_empty());
    assert_eq!(request.fingerprint, identity);
}

fn row_with_median(atom_id: u64, median: Option<f64>) -> Row {
    Row {
        atom_id,
        hydrogen_ids: vec![],
        hydrogen_count: 1,
        radius: None,
        statistics: median.map(|median| Statistics {
            support: 2,
            median,
            standard_deviation: 0.,
            minimum: median,
            maximum: median,
        }),
        limitation: None,
        broad: false,
    }
}

fn report_with_rows(rows: Vec<Row>) -> Report {
    Report {
        nucleus: Nucleus::H1,
        atom_ids: rows.iter().map(|row| row.atom_id).collect(),
        fingerprint: String::new(),
        rows,
        method: METHOD.into(),
        data_version: DATA_VERSION.into(),
        conditions: CONDITIONS.into(),
        limitations: LIMITATIONS.into(),
        attribution: ATTRIBUTION.into(),
    }
}

#[test]
fn row_helpers_distinguish_statistics_from_finite_shifts() {
    let unsupported = row_with_median(1, None);
    assert!(!unsupported.has_statistics());
    assert_eq!(unsupported.shift(), None);

    for median in [-2.5, 0., 8.25] {
        let row = row_with_median(1, Some(median));
        assert!(row.has_statistics());
        assert_eq!(row.shift(), Some(median));
    }

    for median in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let row = row_with_median(1, Some(median));
        assert!(row.has_statistics());
        assert_eq!(row.shift(), None);
    }
}

#[test]
fn report_helpers_count_statistics_and_keep_finite_shifts_in_row_order() {
    let report = report_with_rows(vec![
        row_with_median(70, None),
        row_with_median(42, Some(8.25)),
        row_with_median(10, Some(f64::NAN)),
        row_with_median(10, Some(-2.5)),
        row_with_median(5, Some(f64::INFINITY)),
        row_with_median(3, Some(8.25)),
        row_with_median(1, Some(f64::NEG_INFINITY)),
    ]);

    assert_eq!(report.supported_count(), 6);
    assert_eq!(
        report.shifts().collect::<Vec<_>>(),
        vec![(1, 8.25), (3, -2.5), (5, 8.25)]
    );
}

#[test]
fn report_helpers_handle_reports_without_statistics() {
    for rows in [vec![], vec![row_with_median(1, None)]] {
        let report = report_with_rows(rows);
        assert_eq!(report.supported_count(), 0);
        assert_eq!(report.shifts().next(), None);
    }
}

#[test]
fn the_embedded_index_is_the_source_at_under_60_percent_of_its_size() {
    let source = include_bytes!("../../../../data/nmr/index.tsv");
    let embedded = miniz_oxide::inflate::decompress_to_vec(BUNDLED_TSV).unwrap();
    assert!(embedded == source);
    assert!(BUNDLED_TSV.len() * 10 < source.len() * 6);
}

/// **Export…** writes the same TSV as before scripts could predict: these
/// files were written by [`to_tsv`] before the change.
#[test]
fn the_exported_tsv_is_unchanged() {
    let goldens = [
        (
            ethyl_acetate(),
            Nucleus::H1,
            include_str!("../../../../tests/fixtures/nmr/ethyl-acetate-1H.tsv"),
        ),
        (
            ethyl_acetate(),
            Nucleus::C13,
            include_str!("../../../../tests/fixtures/nmr/ethyl-acetate-carbon-desktop.tsv"),
        ),
        (
            from_smiles("CC(=O)Nc1ccc(O)cc1"),
            Nucleus::H1,
            include_str!("../../../../tests/fixtures/nmr/paracetamol-1H.tsv"),
        ),
        (
            from_smiles("CC(=O)Nc1ccc(O)cc1"),
            Nucleus::C13,
            include_str!("../../../../tests/fixtures/nmr/paracetamol-13C.tsv"),
        ),
    ];
    for (doc, nucleus, golden) in goldens {
        let report = predict(&prepare(&doc, &[]).unwrap(), nucleus).unwrap();
        assert_eq!(to_tsv(&report), golden, "{nucleus:?}");
    }
}

#[test]
fn scripts_get_every_ethyl_acetate_site_with_the_export_metadata() {
    let doc = ethyl_acetate();
    for (nucleus, sites) in [(Nucleus::H1, 3), (Nucleus::C13, 4)] {
        let shifts = predict_drawing(&doc, None, nucleus).unwrap();
        let report = predict(&prepare(&doc, &[]).unwrap(), nucleus).unwrap();
        assert_eq!(shifts.nucleus, nucleus.code());
        assert_eq!(shifts.solvent, "CDCl3");
        assert_eq!(shifts.rows.len(), sites);
        assert_eq!(shifts.credit, ATTRIBUTION);
        assert_eq!(
            (shifts.method.as_str(), shifts.data_version.as_str()),
            (METHOD, DATA_VERSION)
        );
        assert_eq!(
            (shifts.conditions.as_str(), shifts.limitations.as_str()),
            (CONDITIONS, LIMITATIONS)
        );
        assert_eq!(shifts.data_access, DATA_ACCESS);
        for (shift, row) in shifts.rows.iter().zip(&report.rows) {
            let statistics = row.statistics.as_ref().unwrap();
            assert_eq!(shift.atom, row.atom_id);
            assert_eq!(shift.nucleus, nucleus.code());
            assert!((shift.ppm.unwrap() - statistics.median).abs() < 1e-9);
            assert_eq!(shift.references, Some(statistics.support));
            assert_eq!(shift.shell, row.radius);
            assert!(!shift.broad);
            assert_eq!(
                shift.hydrogen_count,
                (nucleus == Nucleus::H1).then_some(row.hydrogen_count)
            );
            assert_eq!(shift.note.as_deref(), row.note(nucleus));
        }
    }
    // IDs are decimal strings on the wire, as every MCP result writes them.
    let value = serde_json::to_value(predict_drawing(&doc, None, Nucleus::H1).unwrap()).unwrap();
    let keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        [
            "conditions",
            "credit",
            "data_access",
            "data_version",
            "limitations",
            "method",
            "nucleus",
            "rows",
            "solvent"
        ]
    );
    let row = &value["rows"][0];
    let keys: Vec<_> = row.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        [
            "atom",
            "broad",
            "hydrogen_count",
            "hydrogens",
            "max",
            "min",
            "note",
            "nucleus",
            "ppm",
            "references",
            "sd",
            "shell"
        ]
    );
    assert!(row["atom"].is_string());
}

#[test]
fn unsupported_sites_are_rows_without_values() {
    let doc = ethanol();
    let ids: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    let protons = predict_drawing(&doc, None, Nucleus::H1).unwrap();
    let ppm: Vec<_> = protons.rows.iter().map(|row| row.ppm.is_some()).collect();
    assert_eq!(ppm, [true, false, false]);
    let hydroxyl = &protons.rows[2];
    assert_eq!(hydroxyl.atom, ids[2]);
    assert_eq!(hydroxyl.hydrogen_count, Some(1));
    assert!(hydroxyl.note.as_deref().unwrap().contains("Exchangeable"));
    assert_eq!(
        (hydroxyl.shell, hydroxyl.references, hydroxyl.sd),
        (None, None, None)
    );
    // A molecule whose sites all lack data is still a result.
    let carbons = predict_drawing(&doc, None, Nucleus::C13).unwrap();
    assert_eq!(carbons.rows.len(), 2);
    assert!(carbons.rows.iter().all(|row| row.ppm.is_none()));
    assert!(carbons.rows.iter().all(|row| row.hydrogen_count.is_none()));
}

#[test]
fn drawn_hydrogens_are_listed_on_their_parent_row() {
    let mut doc = ethanol();
    let carbon = doc.atoms[0].id;
    let hydrogen = doc.add_atom("H", Point::new(-30., 0.));
    doc.add_bond(carbon, hydrogen, 1, "plain");
    let shifts = predict_drawing(&doc, None, Nucleus::H1).unwrap();
    assert_eq!(shifts.rows[0].atom, carbon);
    assert_eq!(shifts.rows[0].hydrogens, [hydrogen]);
    assert_eq!(shifts.rows[0].hydrogen_count, Some(3));
    let value = serde_json::to_value(&shifts.rows[0]).unwrap();
    assert_eq!(
        value["hydrogens"],
        serde_json::json!([hydrogen.to_string()])
    );
}

#[test]
fn a_broad_match_is_flagged_as_data() {
    let shifts = predict_drawing(&from_smiles("CC(=O)Nc1ccc(O)cc1"), None, Nucleus::H1).unwrap();
    let broad: Vec<_> = shifts
        .rows
        .iter()
        .filter(|row| row.broad)
        .map(|row| row.atom)
        .collect();
    assert_eq!(broad, [1]);
    assert_eq!(
        shifts.rows[0].note.as_deref(),
        Some("Broad match · Shell 2 · SD 0.507 ppm")
    );
}

#[test]
fn scripts_predict_exactly_one_molecule() {
    let mut doc = ethanol();
    let ids: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    let other = editing::append(&mut doc, &from_smiles("CC(C)=O"), Point::new(300., 0.));
    assert_eq!(
        predict_drawing(&doc, None, Nucleus::H1).unwrap_err(),
        "NMR prediction needs exactly one molecule; this drawing has 2"
    );
    assert_eq!(
        predict_drawing(&doc, Some(&[ids[0], other[0]]), Nucleus::H1).unwrap_err(),
        "NMR prediction needs exactly one molecule; the given IDs cover 2"
    );
    // Any atom selects its whole molecule.
    let acetone = predict_drawing(&doc, Some(&[other[1]]), Nucleus::C13).unwrap();
    let atoms: Vec<_> = acetone.rows.iter().map(|row| row.atom).collect();
    assert_eq!(atoms, other[..3]);
    assert_eq!(
        predict_drawing(&Document::default(), None, Nucleus::H1).unwrap_err(),
        "NMR prediction needs exactly one molecule; this drawing has 0"
    );
}

#[test]
fn unsupported_chemistry_and_missing_sites_are_errors() {
    for (smiles, nucleus) in [
        ("[Fe]", Nucleus::C13),
        ("[2H]C", Nucleus::H1),
        ("[CH3-]", Nucleus::H1),
    ] {
        assert!(
            predict_drawing(&from_smiles(smiles), None, nucleus).is_err(),
            "{smiles}"
        );
    }
    assert_eq!(
        predict_drawing(&from_smiles("O"), None, Nucleus::C13).unwrap_err(),
        "This molecule has no supported ¹³C sites"
    );
    assert_eq!(
        predict_drawing(&from_smiles("ClC(Cl)(Cl)Cl"), None, Nucleus::H1).unwrap_err(),
        "This molecule has no supported ¹H sites"
    );
}

#[test]
fn label_text_matches_the_show_labels_menu() {
    for (code, format, text) in [
        ("value", LabelFormat::Value, "1.280"),
        ("numbers", LabelFormat::Numbers, "#3"),
        ("ppm", LabelFormat::Ppm, "1.280 ppm"),
        ("both", LabelFormat::Both, "#3 · 1.280 ppm"),
    ] {
        assert_eq!(LabelFormat::from_code(code), Some(format));
        assert_eq!(label_text(format, 3, 1.28, 3), text);
    }
    assert_eq!(LabelFormat::from_code("Both"), None);
}

#[test]
fn shift_labels_default_to_journal_decimals_per_nucleus() {
    assert_eq!(default_decimals(Some(Nucleus::H1)), 2);
    assert_eq!(default_decimals(Some(Nucleus::C13)), 1);
    assert_eq!(default_decimals(None), 2);
    assert_eq!(label_text(LabelFormat::Ppm, 1, 7.2649, 2), "7.26 ppm");
    assert_eq!(label_text(LabelFormat::Ppm, 1, 128.44, 1), "128.4 ppm");
    assert_eq!(label_text(LabelFormat::Both, 1, 128.44, 0), "#1 · 128 ppm");
    assert_eq!(label_text(LabelFormat::Ppm, 1, 1.2, 4), "1.2000 ppm");
    assert_eq!(label_text(LabelFormat::Value, 1, 7.2649, 2), "7.26");
    assert_eq!(label_text(LabelFormat::Value, 1, 128.44, 1), "128.4");
    assert_eq!(label_text(LabelFormat::Value, 1, 128.44, 0), "128");
    let mut doc = ethyl_acetate();
    let before = doc.clone();
    let error = write_labels(&mut doc, &[(1, Some(14.2))], LabelFormat::Ppm, 5);
    assert_eq!(error.unwrap_err(), "decimals must be 0 to 4");
    assert_eq!(doc, before);
}

/// Ethyl acetate's ¹³C rows as scripts pass them.
fn carbon_rows(doc: &Document) -> Vec<(u64, Option<f64>)> {
    let shifts = predict_drawing(doc, None, Nucleus::C13).unwrap();
    shifts.rows.iter().map(|row| (row.atom, row.ppm)).collect()
}

#[test]
fn labels_replace_the_numbers_of_the_listed_atoms_only() {
    use crate::atom_labels::{Number, number_style, shift_label_style};
    let mut doc = ethyl_acetate();
    let rows = carbon_rows(&doc);
    let mut styled = number_style();
    styled.bold = true;
    doc.atom_mut(1).unwrap().display.number = Some(Number {
        text: "a".into(),
        offset: Some(Point::new(4., 4.)),
        style: styled.clone(),
    });
    // The ether O has no ¹³C row.
    let unlisted = Some(Number {
        text: "O".into(),
        offset: Some(Point::new(1., 1.)),
        style: number_style(),
    });
    doc.atom_mut(3).unwrap().display.number = unlisted.clone();
    let labelled = write_labels(&mut doc, &rows, LabelFormat::Both, 1).unwrap();
    assert_eq!(labelled, [1, 2, 4, 5]);
    let number = |id| doc.atom(id).unwrap().display.number.clone().unwrap();
    assert_eq!(number(1).text, "#1 · 14.2 ppm");
    assert_eq!((number(1).offset, number(1).style), (None, styled));
    assert_eq!(number(4).text, "#4 · 170.7 ppm");
    // Without a style of its own, a label is smaller than the atom labels.
    assert_eq!(number(4).style, shift_label_style(&doc));
    assert!(number(4).style.size_pt < doc.drawing_style.font_size_pt * 0.7);
    assert_eq!(doc.atom(3).unwrap().display.number, unlisted);
    doc.validate().unwrap();
    assert!(crate::scene::svg(&doc).contains(">#4 · 170.7 ppm</text>"));
    // CDXML keeps plain atom numbers: ChemDraw places them, without leaders.
    let cdxml = crate::exchange::drawing::write(&doc, Default::default()).unwrap();
    assert!(cdxml.contains("AtomNumber=\"#4 · 170.7 ppm\""), "{cdxml}");
}

#[test]
fn rows_without_a_shift_are_skipped() {
    let mut doc = ethyl_acetate();
    let labelled = write_labels(
        &mut doc,
        &[(1, None), (2, Some(61.05))],
        LabelFormat::Ppm,
        3,
    );
    assert_eq!(labelled.unwrap(), [2]);
    assert_eq!(doc.atom(1).unwrap().display.number, None);
    let number = doc.atom(2).unwrap().display.number.clone().unwrap();
    assert_eq!(number.text, "61.050 ppm");
}

#[test]
fn a_bad_row_anywhere_changes_nothing() {
    let doc = ethyl_acetate();
    let first = (1, Some(14.2));
    for (row, error) in [
        ((99, Some(1.)), "Atom 99 is not in the drawing"),
        ((99, None), "Atom 99 is not in the drawing"),
        (
            (2, Some(f64::NAN)),
            "The ppm of atom 2 is not a finite number",
        ),
        (
            (2, Some(f64::INFINITY)),
            "The ppm of atom 2 is not a finite number",
        ),
        // "#2 · 100000000000000000000.00 ppm" is 33 characters.
        (
            (2, Some(1e20)),
            "The label of atom 2 is longer than 32 characters",
        ),
    ] {
        let mut copy = doc.clone();
        let result = write_labels(&mut copy, &[first, row], LabelFormat::Both, 2);
        assert_eq!(result.unwrap_err(), error);
        assert_eq!(copy, doc);
    }
}

#[test]
fn labelled_drawings_keep_their_version_floor_through_rsk() {
    let mut raised = ethyl_acetate();
    raised.drawing_style.crossing_clearance_pt += 0.5;
    assert!(raised.file_version() > crate::document::DISTANCE_BONDS);
    for doc in [ethyl_acetate(), raised] {
        let mut labelled = doc.clone();
        write_labels(&mut labelled, &carbon_rows(&doc), LabelFormat::Numbers, 1).unwrap();
        assert_eq!(labelled.file_version(), doc.file_version());
        let reopened = Document::from_native_file(&labelled.file_json().unwrap()).unwrap();
        assert_eq!(reopened.version, doc.file_version());
        for atom in &labelled.atoms {
            let number = &reopened.atom(atom.id).unwrap().display.number;
            assert_eq!(number, &atom.display.number);
        }
        let number = reopened.atom(5).unwrap().display.number.clone().unwrap();
        assert_eq!(number.text, "#5");
    }
}
