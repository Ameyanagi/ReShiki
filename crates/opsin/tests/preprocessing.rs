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
