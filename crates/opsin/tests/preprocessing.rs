use opsin::preprocess::preprocess;

#[test]
fn upstream_preprocessor_cases() {
    for (input, expected) in [
        ("$a-bromo", "alpha-bromo"),
        ("$b-bromo", "beta-bromo"),
        ("$g-bromo", "gamma-bromo"),
        ("$d-bromo", "delta-bromo"),
        ("$e-bromo", "epsilon-bromo"),
        ("$l-bromo", "lambda-bromo"),
        ("α-bromo", "alpha-bromo"),
        ("sulphur dioxide", "sulfur dioxide"),
        (".alpha.-methyl-toluene", "alpha-methyl-toluene"),
        (".alpha..beta..eta.", "alphabetaeta"),
        ("&alpha;-methyl-toluene", "alpha-methyl-toluene"),
        ("&BETA;-methyl-styrene", "beta-methyl-styrene"),
        ("  ethanol \n", "ethanol"),
        ("ethan–1–ol", "ethan-1-ol"),
        ("N′,N″-dimethyl", "N',N''-dimethyl"),
        ("²H₂O", "2H2O"),
        ("SulPHur", "sulfur"),
        ("\u{a0}ethanol\u{a0}", " ethanol "),
        ("\u{feff}ethanol", "ethanol"),
        (".fwdarw.", "->"),
        ("(S)‐lactic acid", "(S)-lactic acid"),
    ] {
        assert_eq!(preprocess(input).unwrap(), expected, "{input:?}");
    }
    assert_eq!(
        preprocess("").unwrap_err().to_string(),
        "Input chemical name was blank!"
    );
    assert!(
        preprocess("💥ethanol")
            .unwrap_err()
            .to_string()
            .starts_with("Unrecognised unicode character:")
    );
}

#[test]
fn em_dash_separates_components() {
    for separator in ["—", " —", "— ", " — ", " \t—\u{a0} "] {
        let input = format!("benzene{separator}2-aminopyridine");
        assert_eq!(preprocess(&input).unwrap(), "benzene - 2-aminopyridine");
    }
}

#[test]
fn repeated_whitespace_is_normalized() {
    assert_eq!(
        preprocess("sodium  \t\n\u{b}\u{c}\r \u{a0}\u{3000}chloride").unwrap(),
        "sodium chloride"
    );
    assert_eq!(
        preprocess("sodium \u{ad} chloride").unwrap(),
        "sodium chloride"
    );
}

#[test]
fn terminal_em_dash_remains_a_hyphen() {
    for input in ["ethyl—", "ethyl— \t"] {
        assert_eq!(preprocess(input).unwrap(), "ethyl-");
    }
}

#[test]
fn other_dashes_remain_hyphens() {
    for dash in [
        '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2015}', '\u{2212}',
    ] {
        let input = format!("2{dash}methylpyridine");
        assert_eq!(preprocess(&input).unwrap(), "2-methylpyridine");
    }
}
