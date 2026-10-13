use super::{Script, label_runs};

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
