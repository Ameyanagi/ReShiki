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

#[test]
fn upstream_superscript_digits() {
    for (input, expected) in [
        ("1\u{2074}-ol", "1^4-ol"),
        ("4\u{b9}\u{2070},5\u{b2}-diol", "4^10,5^2-diol"),
        ("3\u{b9}-yl", "3^1-yl"),
    ] {
        assert_eq!(preprocess(input).unwrap(), expected, "{input}");
    }
}

#[test]
fn upstream_html_superscripts() {
    for input in ["1<sup>4</sup>-ol", "1<SUP>4</SUP>-ol"] {
        assert_eq!(preprocess(input).unwrap(), "1^4-ol");
    }
}

#[test]
fn superscript_replacements_observe_original_input_context() {
    for (input, expected) in [
        ("\u{b2}\u{b3}H", "23H"),
        ("\u{b2}H", "2H"),
        (
            "1\u{2070}\u{b9}\u{b2}\u{b3}\u{2074}\u{2075}\u{2076}\u{2077}\u{2078}\u{2079}",
            "1^0123456789",
        ),
        ("N<SuP>2</sUp>", "N^2"),
        ("1<\u{17f}up>4</\u{17f}up>", "1^4"),
        ("1<sup>4</sup>\u{b2}", "1^42"),
        ("</sup>1", "1"),
    ] {
        assert_eq!(preprocess(input).unwrap(), expected, "{input}");
    }
}
