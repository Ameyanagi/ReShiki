use super::*;

// Independently authored rule cases. The names assert parent selection,
// functional seniority, complete locants and exact ring topology; OPSIN is not
// involved in this suite and cannot make a wrong locant preference pass it.
pub(crate) const CASES: &[(&str, &str)] = &[
    ("C", "methane"),
    ("CC", "ethane"),
    ("CCO", "ethan-1-ol"),
    ("CC(C)(C)CCC(C)C", "2,2,5-trimethylhexane"),
    ("CCCCCCCCCCC", "undecane"),
    ("CC(Cl)C(Br)C", "2-bromo-3-chlorobutane"),
    ("C=CCO", "prop-2-en-1-ol"),
    ("CC=CC", "but-2-ene"),
    ("CC#CC", "but-2-yne"),
    ("CC(=O)O", "ethanoic acid"),
    ("CC(O)C(=O)O", "2-hydroxypropanoic acid"),
    ("NCC(=O)O", "2-aminoethanoic acid"),
    ("CC(N)C(O)C(=O)O", "3-amino-2-hydroxybutanoic acid"),
    ("CC(=O)CC(=O)O", "3-oxobutanoic acid"),
    ("CCOC(=O)C", "ethyl ethanoate"),
    ("CC(C)OC(=O)C", "propan-2-yl ethanoate"),
    ("CCOC(=O)C(C)C", "ethyl 2-methylpropanoate"),
    ("CC(=O)N", "ethanamide"),
    ("OCCC(=O)N", "3-hydroxypropanamide"),
    ("CC#N", "ethanenitrile"),
    ("O=CCCC#N", "4-oxobutanenitrile"),
    ("CCC=O", "propanal"),
    ("C=O", "methanal"),
    ("CC(=O)C", "propan-2-one"),
    ("CC(O)CO", "propane-1,2-diol"),
    ("CCCN", "propan-1-amine"),
    ("COCC", "1-methoxyethane"),
    ("CCC(OC)CC", "3-methoxypentane"),
    ("CCOCCC", "1-ethoxypropane"),
    ("CCCOCCCC", "1-propoxybutane"),
    ("CCCCOCCCCC", "1-butoxypentane"),
    ("CC(C)OCCCC", "1-(propan-2-yloxy)butane"),
    ("CC(C)COCCCC", "1-(2-methylpropoxy)butane"),
    ("CCCCCOCCCCCC", "1-(pentyloxy)hexane"),
    ("C1CCCCC1", "cyclohexane"),
    ("CC1CCCCC1", "1-methylcyclohexane"),
    ("CC(C)CC1CCCCC1", "1-(2-methylpropyl)cyclohexane"),
    (
        "CCC1CC(CC(C)C)CCC1",
        "1-ethyl-3-(2-methylpropyl)cyclohexane",
    ),
    ("OC1CCCCC1", "cyclohexan-1-ol"),
    ("O=C1CCCCC1", "cyclohexan-1-one"),
    ("OC1CCCCC1O", "cyclohexane-1,2-diol"),
    ("c1ccccc1", "benzene"),
    ("Oc1ccccc1", "phenol"),
    ("Nc1ccccc1", "aniline"),
    ("Oc1ccc(C)cc1", "4-methylphenol"),
    ("Nc1cc(Cl)ccc1", "3-chloroaniline"),
    ("c1cc(Cl)c(Br)cc1", "1-bromo-2-chlorobenzene"),
    ("n1ccccc1", "pyridine"),
    ("Cc1ccncc1", "4-methylpyridine"),
    ("Cc1cccnn1", "3-methylpyridazine"),
    ("n1ncccc1", "pyridazine"),
    ("n1cnccc1", "pyrimidine"),
    ("n1ccncc1", "pyrazine"),
    ("[nH]1nccc1", "1H-pyrazole"),
    ("[nH]1nccc1C", "5-methyl-1H-pyrazole"),
    ("[nH]1cncc1", "1H-imidazole"),
    ("c1ccoc1", "furan"),
    ("Cc1ccco1", "2-methylfuran"),
    ("c1ccsc1", "thiophene"),
    ("c1cscn1", "1,3-thiazole"),
    ("Cc1nccs1", "2-methyl-1,3-thiazole"),
    ("C[C@@H](O)C(=O)O", "(2R)-2-hydroxypropanoic acid"),
    ("C[C@H](O)C(=O)O", "(2S)-2-hydroxypropanoic acid"),
    ("C/C=C/C", "(2E)-but-2-ene"),
    ("C/C=C\\C", "(2Z)-but-2-ene"),
    ("C[C@@H](O)[C@@H](O)C", "(2R,3S)-butane-2,3-diol"),
];

#[test]
fn independent_rules_cover_parent_seniority_locants_unsaturation_rings_and_cip()
-> Result<(), String> {
    for &(smiles, expected) in CASES {
        assert_eq!(
            generate(smiles).map_err(|e| format!("{smiles}: {e}"))?,
            expected,
            "{smiles}"
        );
    }
    Ok(())
}
#[test]
fn graph_permutations_and_hydrogen_spelling_do_not_change_name() -> Result<(), String> {
    for alternatives in [
        vec![
            "CCO",
            "OCC",
            "[CH3][CH2][OH]",
            "[H]OC([H])([H])C([H])([H])[H]",
        ],
        vec!["CC(C)(C)CCC(C)C", "CC(C)CCC(C)(C)C", "C(C)(C)(C)CCC(C)C"],
        vec!["CC(O)C(=O)O", "OC(=O)C(O)C", "C(C)(O)C(O)=O"],
        vec!["Oc1ccc(C)cc1", "Cc1ccc(O)cc1", "OC1=CC=C(C)C=C1"],
        vec!["C/C=C/C", "C(\\C)=C/C"],
    ] {
        let name = generate(at(&alternatives, 0)?)?;
        for text in alternatives {
            assert_eq!(generate(text)?, name, "{text}");
        }
    }
    Ok(())
}
#[test]
fn unsupported_features_never_become_neutral_unlabeled_or_simplified_names() {
    for smiles in [
        "[13CH3]CO",
        "CC(=O)[O-]",
        "[NH4+]",
        "C1CC2CCC1C2",
        "C1CCC2(C1)CCCCC2",
        "c1ccc2ccccc2c1",
        "CS",
        "CN(C)C",
        "CC(=O)NC",
        "CC(=O)OC(=O)C",
        "C1=CCCCC1",
        "COOC",
        "*C",
        "[CH3]",
        "[Na+].[Cl-]",
        "CCCCCC(C[C@H](F)Cl)CCCCCC",
    ] {
        assert!(
            generate(smiles).is_err(),
            "Unsupported {smiles} received a name"
        );
    }
    assert!(generate(&"C".repeat(21)).is_err());
    assert!(generate(&"C".repeat(65)).is_err());
}
#[test]
fn work_exhaustion_is_failure_instead_of_partial_parent_selection() {
    let mut c = Context::new("CC(C)(C)CCC(C)C").unwrap();
    c.work = 1;
    assert!(
        parent::candidates(&mut c)
            .unwrap_err()
            .contains("work budget")
    );
}
