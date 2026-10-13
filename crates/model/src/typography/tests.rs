use super::{
    Script, ScriptRun, effective_label_runs, label_runs, normalize_label_scripts,
    validate_label_scripts,
};

#[test]
fn free_label_formula_runs_cover_the_golden_table() {
    use Script::{Normal as N, Subscript as B, Superscript as P};
    type Run = (std::ops::Range<usize>, Script);
    let cases: &[(&str, &[Run])] = &[
        ("M2+", &[(0..1, N), (1..3, P)]),
        ("Pd2+", &[(0..2, N), (2..4, P)]),
        ("R1", &[(0..1, N), (1..2, B)]),
        ("L2", &[(0..1, N), (1..2, B)]),
        ("SO42-", &[(0..2, N), (2..3, B), (3..5, P)]),
        ("SO4 2-", &[(0..2, N), (2..3, B), (3..6, P)]),
        ("CO32-", &[(0..2, N), (2..3, B), (3..5, P)]),
        ("NR4+", &[(0..2, N), (2..3, B), (3..4, P)]),
        ("NMe4+", &[(0..3, N), (3..4, B), (4..5, P)]),
        ("PPh4+", &[(0..3, N), (3..4, B), (4..5, P)]),
        ("Me3Si+", &[(0..2, N), (2..3, B), (3..5, N), (5..6, P)]),
        ("X", &[(0..1, N)]),
        ("Ar", &[(0..2, N)]),
        ("L", &[(0..1, N)]),
        ("Cl,Br", &[(0..5, N)]),
        ("2 eq", &[(0..4, N)]),
        ("C12", &[(0..1, N), (1..3, B)]),
        ("[M]2+", &[(0..3, N), (3..5, P)]),
        ("+", &[(0..1, N)]),
        ("2-", &[(0..2, N)]),
        ("R₁", &[(0..4, N)]),
        ("custom ligand", &[(0..13, N)]),
        ("Fe 3+", &[(0..2, N), (2..5, P)]),
        ("M2−", &[(0..1, N), (1..5, P)]),
        ("M2⁺", &[(0..1, N), (1..5, P)]),
        ("M2⁻", &[(0..1, N), (1..5, P)]),
        ("(NR)4+", &[(0..4, N), (4..5, B), (5..6, P)]),
        ("C0+", &[(0..1, N), (1..2, B), (2..3, P)]),
        ("R,12", &[(0..4, N)]),
        ("R-12", &[(0..4, N)]),
        ("Fe  3+", &[(0..6, N)]),
        ("R1 ligand+", &[(0..10, N)]),
        ("", &[]),
    ];
    for (text, expected) in cases {
        let runs = label_runs(text);
        assert_eq!(runs, *expected, "{text}");
        let mut covered = 0;
        for (range, _) in &runs {
            assert_eq!(range.start, covered, "{text}");
            assert!(text.is_char_boundary(range.start));
            assert!(text.is_char_boundary(range.end));
            covered = range.end;
        }
        assert_eq!(covered, text.len(), "{text}");
        for pair in runs.windows(2) {
            assert_ne!(pair[0].1, pair[1].1, "Unmerged runs for {text}");
        }
    }
}

#[test]
fn manual_label_runs_override_inference_and_merge_normal_gaps() -> Result<(), String> {
    let mut display = crate::atom_labels::AtomDisplay {
        variable: Some("SO42-".into()),
        ..Default::default()
    };
    assert_eq!(effective_label_runs(&display), label_runs("SO42-"));
    display.label_scripts = vec![ScriptRun {
        start: 2,
        end: 3,
        script: Script::Superscript,
    }];
    assert_eq!(
        effective_label_runs(&display),
        vec![
            (0..2, Script::Normal),
            (2..3, Script::Superscript),
            (3..5, Script::Normal),
        ]
    );
    display.label_scripts = vec![ScriptRun {
        start: 1,
        end: 3,
        script: Script::Normal,
    }];
    assert_eq!(effective_label_runs(&display), vec![(0..5, Script::Normal)]);
    display.set_variable(Some("SO42-".into()));
    assert!(display.label_scripts.is_empty());
    assert_eq!(effective_label_runs(&display), label_runs("SO42-"));
    display.label_scripts.push(ScriptRun {
        start: 2,
        end: 3,
        script: Script::Subscript,
    });
    display.set_variable(None);
    assert!(display.label_scripts.is_empty());
    assert!(effective_label_runs(&display).is_empty());
    display.validate()?;
    Ok(())
}

#[test]
fn manual_label_run_normalization_preserves_literal_intent() -> Result<(), String> {
    let run = |start, end, script| ScriptRun { start, end, script };
    assert_eq!(
        normalize_label_scripts(
            "NH42+",
            &[
                run(0, 2, Script::Normal),
                run(2, 2, Script::Subscript),
                run(2, 3, Script::Subscript),
                run(3, 4, Script::Superscript),
                run(4, 5, Script::Superscript),
            ]
        )?,
        vec![run(2, 3, Script::Subscript), run(3, 5, Script::Superscript)]
    );
    assert_eq!(
        normalize_label_scripts("R1", &[run(1, 2, Script::Normal)])?,
        vec![run(0, 2, Script::Normal)]
    );
    assert!(normalize_label_scripts("R1", &[])?.is_empty());
    assert!(normalize_label_scripts("R1", &[run(1, 1, Script::Normal)])?.is_empty());
    Ok(())
}

#[test]
fn stored_manual_label_ranges_reject_invalid_offsets_and_missing_text() {
    let run = |start, end| ScriptRun {
        start,
        end,
        script: Script::Subscript,
    };
    for runs in [
        vec![run(0, 0)],
        vec![run(2, 1)],
        vec![run(0, 5)],
        vec![run(0, 2)],
        vec![run(2, 4)],
        vec![run(1, 4), run(0, 1)],
        vec![run(0, 4), run(1, 4)],
    ] {
        let display = crate::atom_labels::AtomDisplay {
            variable: Some("R₁".into()),
            label_scripts: runs.clone(),
            ..Default::default()
        };
        assert!(display.validate().is_err(), "{runs:?}");
        assert!(validate_label_scripts("R₁", &runs).is_err(), "{runs:?}");
    }
    for runs in [vec![run(2, 1)], vec![run(0, 2)], vec![run(1, 4), run(0, 1)]] {
        assert!(normalize_label_scripts("R₁", &runs).is_err());
    }
    let display = crate::atom_labels::AtomDisplay {
        label_scripts: vec![run(0, 1)],
        ..Default::default()
    };
    assert_eq!(
        display.validate(),
        Err("Atom label scripts require a variable label".into())
    );
    let display = crate::atom_labels::AtomDisplay {
        variable: Some("R₁".into()),
        label_scripts: vec![run(1, 4)],
        ..Default::default()
    };
    assert!(display.validate().is_ok());
    assert_eq!(
        effective_label_runs(&display),
        vec![(0..1, Script::Normal), (1..4, Script::Subscript)]
    );
}
