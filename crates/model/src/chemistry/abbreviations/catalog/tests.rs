use super::*;

const CATEGORIES: &[&str] = &[
    "alkyl",
    "alkoxy",
    "aryl",
    "heteroaryl",
    "protecting group",
    "silyl",
    "carbonyl",
    "sulfonyl",
    "sulfur",
    "phosphorus",
    "boron",
    "metal",
    "nitrogen",
    "halogen",
];

#[test]
fn bundled_entries_are_complete_and_rdkit_comparable() {
    let (presets, errors) = parse(BUILTIN, false);
    assert!(errors.is_empty(), "{errors:#?}");
    let file: File = serde_json::from_str(BUILTIN).unwrap();
    assert_eq!(file.groups.len(), presets.len());
    for (entry, preset) in file.groups.iter().zip(&presets) {
        let label = &entry.label;
        // Every bundled formula is written independently of the SMILES and
        // checked by from_entry, so a SMILES typo cannot slip through.
        assert!(entry.formula.is_some(), "{label} needs a formula");
        assert!(
            CATEGORIES.contains(&entry.category.as_str()),
            "{label}: unknown category {}",
            entry.category
        );
        assert!(!entry.names.is_empty(), "{label} needs a full name");
        // Bracket hydrogens become RDKit query predicates that the native
        // matcher does not model, so bundled SMILES avoid them.
        let prepared = smiles::prepare(&entry.smiles).unwrap();
        assert!(
            prepared
                .state
                .graph
                .atoms
                .iter()
                .all(|atom| atom.explicit_hydrogens == 0),
            "{label}: write hydrogens implicitly"
        );
        assert_eq!(
            preset.smiles.matches('*').count(),
            1,
            "{label}: one attachment"
        );
        assert!(preset.positions().is_ok(), "{label}: layout");
    }
}

/// A bracket hydrogen ([nH]) is a hydrogen-count query, as in RDKit, so a
/// neutral NH ring matches neither its tautomer nor its anion.
#[test]
fn bracket_hydrogens_are_matched_as_hydrogen_counts() {
    let entry = Entry {
        label: "Im4".into(),
        reverse: String::new(),
        smiles: "*c1nc[nH]c1".into(),
        formula: None,
        category: String::new(),
        names: Vec::new(),
        aliases: Vec::new(),
        contract: true,
    };
    let preset = Preset::from_entry(entry, true).unwrap();
    for (smiles, expected) in [
        ("Cc1nc[nH]c1", true),
        ("Cc1[nH]cnc1", false),
        ("Cc1nc[n-]c1", false),
    ] {
        let state = smiles::prepare(smiles).unwrap().state;
        let hydrogens = matcher::total_hydrogens(&state);
        let target = matcher::Target::new(&state.graph, &state.rings, &hydrogens).unwrap();
        let found = target
            .matches(&preset, &mut matcher::Work::default())
            .unwrap();
        assert_eq!(!found.is_empty(), expected, "{smiles}");
    }
}

#[test]
fn formulas_are_hill_order_substituents() {
    for (smiles, formula) in [
        ("*C", "CH3"),
        ("*C(F)(F)F", "CF3"),
        ("*[N+](=O)[O-]", "NO2"),
        ("*O[Si](C)(C)C(C)(C)C", "C6H15OSi"),
        ("*S(=O)(=O)C(F)(F)F", "CF3O2S"),
        ("*[N+](C)(C)C", "C3H9N+"),
        ("*C(=O)[O-]", "CO2-"),
        ("*Br", "Br"),
    ] {
        let prepared = smiles::prepare(smiles).unwrap();
        assert_eq!(substituent_formula(&prepared).unwrap(), formula, "{smiles}");
    }
}

#[test]
fn invalid_entries_are_reported_and_valid_ones_kept() {
    let text = r#"{"format": 1, "groups": [
        {"label": "Good", "smiles": "*CC(C)(C)C", "formula": "C5H11"},
        {"label": "Two", "smiles": "*CC*"},
        {"label": "Late", "smiles": "C*"},
        {"label": "Bad", "smiles": "*C(("},
        {"label": "Wrong", "smiles": "*CC", "formula": "C3H7"},
        {"label": "Good", "smiles": "*C"},
        {"label": "has space", "smiles": "*C"},
        {"label": "Chiral", "smiles": "*[C@H](F)Cl"},
        {"label": "Alias", "smiles": "*CC", "aliases": ["Good"]},
        {"label": "Ring", "smiles": "*C1CC1", "contract": false},
        {"label": "Double", "smiles": "*=CC"},
        {"label": "Radical", "smiles": "*[CH2]"},
        {"label": "A,B", "smiles": "*C"},
        {"label": "Heavy", "smiles": "*[13CH3]"},
        {"label": "Typo", "smile": "*C"},
        {"label": "Late2", "smiles": "*CC"},
        {"label": "Azide", "smiles": "*C", "aliases": ["N3"]},
        {"label": "GoodO", "smiles": "*CC", "reverse": "Late2"},
        {"label": "ExplicitH", "smiles": "*C[H]"}
    ]}"#;
    let (presets, errors) = parse(text, true);
    let labels: Vec<_> = presets.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["Good", "Ring", "Late2"]);
    assert!(presets.iter().all(|p| p.user));
    assert!(!presets[1].contract);
    assert_eq!(errors.len(), 16, "{errors:#?}");
    for (error, expected) in errors.iter().zip([
        "Group 2 (Two): SMILES must start with the single *",
        "Group 3 (Late): SMILES must start",
        "Group 4 (Bad): Invalid SMILES",
        "Group 5 (Wrong): Formula C3H7 does not match the SMILES (C2H5)",
        "Group 6 (Good): \"Good\" is defined twice",
        "Group 7 (has space): Label \"has space\"",
        "Group 8 (Chiral): Stereo",
        "Group 9 (Alias): \"Good\" is defined twice",
        "Group 11 (Double): The * attachment must use a single bond",
        "Group 12 (Radical): Radicals are not supported",
        "Group 13 (A,B): Label \"A,B\" must be 1-32 characters without spaces or commas",
        "Group 14 (Heavy): Isotopes are not supported",
        "Group 15 (Typo): unknown field `smile`",
        "Group 17 (Azide): \"N3\" already means an element or a built-in group",
        "Group 18 (GoodO): \"Late2\" is defined twice",
        "Group 19 (ExplicitH): Write hydrogens implicitly",
    ]) {
        assert!(error.starts_with(expected), "{error}");
    }
    assert!(parse("{", true).1[0].starts_with("Invalid catalog file"));
    assert!(parse(r#"{"format": 2, "groups": []}"#, true).1[0].contains("format 2"));
}

#[test]
fn user_groups_win_label_clashes_and_keep_the_bundled_order() {
    let builtin = builtin().unwrap();
    let text = r#"{"format": 1, "groups": [
        {"label": "TBS", "smiles": "*[Si](CC)(CC)CC"},
        {"label": "Mine", "smiles": "*CC(F)(F)F", "aliases": ["TBDMS"]}
    ]}"#;
    let (user, errors) = parse(text, true);
    assert!(errors.is_empty(), "{errors:?}");
    let merged = merge(builtin, user);
    assert_eq!(merged.len(), builtin.len() + 1);
    let position = |catalog: &[Arc<Preset>]| catalog.iter().position(|p| p.label == "TBS");
    assert_eq!(position(&merged), position(builtin));
    let (tbs, alias) = lookup_in(&merged, "TBS").unwrap();
    assert!(tbs.user && !alias);
    assert_eq!(tbs.smiles, "*[Si](CC)(CC)CC");
    // A user alias wins over the bundled alias with the same spelling.
    let (mine, alias) = lookup_in(&merged, "TBDMS").unwrap();
    assert!(mine.user && alias);
    assert_eq!(mine.label, "Mine");
    assert_eq!(merged.last().unwrap().label, "Mine");
    assert!(lookup_in(builtin, "Mine").is_none());
}

/// Typing resolves these before the catalog, so a user group could be saved
/// but never created under them.
#[test]
fn user_groups_cannot_take_reserved_spellings() {
    for spelling in ["N3", "MgBr", "Cp", "OCH3", "C2H5", "Fe", "Np", "*"] {
        let text = format!(
            r#"{{"format": 1, "groups": [{{"label": "{spelling}", "smiles": "*OC"}},
            {{"label": "Fine", "smiles": "*OC", "aliases": ["{spelling}"]}}]}}"#
        );
        let (presets, errors) = parse(&text, true);
        assert!(presets.is_empty(), "{spelling}");
        assert_eq!(errors.len(), 2, "{spelling}: {errors:?}");
        assert!(errors[0].contains("already means"), "{errors:?}");
    }
}
