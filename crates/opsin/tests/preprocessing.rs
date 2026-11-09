// SPDX-License-Identifier: MIT
// Origin: adapted
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/PreProcessor.java | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/StringTools.java | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Source: OPSIN | opsin-core/src/test/java/uk/ac/cam/ch/wwmm/opsin/PreProcessorTest.java | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Source: ReShiki | crates/opsin/tests/preprocessing.rs | 0e6e9009ca2e8bfebe149c260fc5a2f484f7dfb9
// Copyright 2017 Daniel Lowe
// Changes: Rust graph/tree construction and regression harness around source-derived cases/assertions; local regressions retained.

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
        ("(2Ξ,3R)-3-chlorobutan-2-ol", "(2Xi,3R)-3-chlorobutan-2-ol"),
        (
            "(1ξ,4ξ)-cyclohexane-1,4-diol",
            "(1xI,4xI)-cyclohexane-1,4-diol",
        ),
        ("(2Ξ,3ξ,4Xi,5xI,6XI,7xi)", "(2Xi,3xI,4xi,5xi,6xi,7xi)"),
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
fn ascii_xi_normalization_recognizes_descriptors_without_changing_word_text() {
    for (input, expected) in [
        ("(Xi,xI,XI,xi)", "(xi,xi,xi,xi)"),
        ("[Xi] {xI} XI-xi", "[xi] {xi} xi-xi"),
        ("(4aXi,4a'XI,4'aXi)", "(4axi,4a'xi,4'axi)"),
        ("5Xi-cholestane", "5xi-cholestane"),
        ("5XIcholestane", "5xicholestane"),
        ("(Ξ,ξ,Xi,xI)", "(Xi,xI,xi,xi)"),
        ("OXIME", "OXIME"),
        ("THIOOXIME", "THIOOXIME"),
        ("XIPAMIDE", "XIPAMIDE"),
        (
            "4-NITROBENZALDEHYDE S-(4-NITROPHENYL)THIOOXIME",
            "4-NITROBENZALDEHYDE S-(4-NITROPHENYL)THIOOXIME",
        ),
    ] {
        assert_eq!(preprocess(input).unwrap(), expected, "{input}");
    }
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

#[test]
fn cage_suffix_stereochemistry_shares_the_suffix_locant() {
    for (name, expected) in [
        (
            "bicyclo[2.2.1]heptan-2-exo-ol",
            "(2-exo)-bicyclo[2.2.1]heptan-2-ol",
        ),
        (
            "8-azabicyclo[3.2.1]octan-3-endo-ol",
            "(3-endo)-8-azabicyclo[3.2.1]octan-3-ol",
        ),
        (
            "bicyclo[2.2.2]octan-2-exo-ol",
            "bicyclo[2.2.2]octan-2-exo-ol",
        ),
        (
            "bicyclo[3.2.1]octane-3-endo,8-syn-diol",
            "(3-endo,8-syn)-bicyclo[3.2.1]octane-3,8-diol",
        ),
        (
            "bicyclo[3.2.1]octane-3-endo,8-diol",
            "(3-endo)-bicyclo[3.2.1]octane-3,8-diol",
        ),
        (
            "3-endo,8-syn-dimethylbicyclo[3.2.1]octane",
            "(3-endo,8-syn)-3,8-dimethylbicyclo[3.2.1]octane",
        ),
        (
            "2,3-dichloro-5-exo,6-endo-dimethylbicyclo[2.2.1]heptane",
            "(5-exo,6-endo)-2,3-dichloro-5,6-dimethylbicyclo[2.2.1]heptane",
        ),
        (
            "(2-exo,3-endo)-bicyclo[2.2.1]heptane-2,3-diol",
            "(2-exo,3-endo)-bicyclo[2.2.1]heptane-2,3-diol",
        ),
        (
            "8-methyl-8-azabicyclo[3.2.1]octan-3-endo-yl acetate",
            "(3-endo)-8-methyl-8-azabicyclo[3.2.1]octan-3-yl acetate",
        ),
    ] {
        assert_eq!(preprocess(name).unwrap(), expected);
    }
}

#[test]
fn cage_descriptors_stay_in_their_enclosing_group() {
    for (name, expected) in [
        (
            "2-(bicyclo[2.2.1]heptan-2-exo-yl)-3-(bicyclo[2.2.1]heptan-2-exo-yl)propan-1-ol",
            "2-((2-exo)-bicyclo[2.2.1]heptan-2-yl)-3-((2-exo)-bicyclo[2.2.1]heptan-2-yl)propan-1-ol",
        ),
        (
            "2-(bicyclo[2.2.1]heptan-2-exo-yl)-3-(bicyclo[2.2.1]heptan-2-endo-yl)propan-1-ol",
            "2-((2-exo)-bicyclo[2.2.1]heptan-2-yl)-3-((2-endo)-bicyclo[2.2.1]heptan-2-yl)propan-1-ol",
        ),
        (
            "2-(bicyclo[2.2.2]octan-2-exo-yl)-3-(bicyclo[2.2.1]heptan-2-yl)propan-1-ol",
            "2-(bicyclo[2.2.2]octan-2-exo-yl)-3-(bicyclo[2.2.1]heptan-2-yl)propan-1-ol",
        ),
        (
            "2-[3-(bicyclo[2.2.1]heptan-2-endo-yl)propyl]bicyclo[2.2.1]heptan-2-exo-ol",
            "(2-exo)-2-[3-((2-endo)-bicyclo[2.2.1]heptan-2-yl)propyl]bicyclo[2.2.1]heptan-2-ol",
        ),
        (
            "2-(3-endo,8-syn-dimethylbicyclo[3.2.1]octan-2-yl)ethan-1-ol",
            "2-((3-endo,8-syn)-3,8-dimethylbicyclo[3.2.1]octan-2-yl)ethan-1-ol",
        ),
        (
            "2-[bicyclo[2.2.1]heptan-2-exo-yl]ethan-1-ol",
            "2-[(2-exo)-bicyclo[2.2.1]heptan-2-yl]ethan-1-ol",
        ),
    ] {
        assert_eq!(preprocess(name).unwrap(), expected, "{name}");
        assert_eq!(preprocess(expected).unwrap(), expected, "{expected}");
    }
}
