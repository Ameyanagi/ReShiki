//! Complete peptide-symbol sequences become ordinary OPSIN amino-acid names.
//! Only backbone amide links are implied; side-chain notation is left to OPSIN.
use super::Expansion;
use crate::naming::Error;

struct Residue {
    symbol: &'static str,
    acid: &'static str,
    acyl: &'static str,
    amide: &'static str,
    achiral: bool,
}

// The acyl spellings are significant: e.g. glutaminyl/cysteinyl retain "in",
// while glutamyl denotes the acid. Alpha-asparagine and alpha-glutamine put a
// terminal amide on C-1 while retaining the Asp/Glu side-chain carboxyl group.
const RESIDUES: &[Residue] = &[
    Residue {
        symbol: "Ala",
        acid: "alanine",
        acyl: "alanyl",
        amide: "alaninamide",
        achiral: false,
    },
    Residue {
        symbol: "Arg",
        acid: "arginine",
        acyl: "arginyl",
        amide: "argininamide",
        achiral: false,
    },
    Residue {
        symbol: "Asn",
        acid: "asparagine",
        acyl: "asparaginyl",
        amide: "asparaginamide",
        achiral: false,
    },
    Residue {
        symbol: "Asp",
        acid: "aspartic acid",
        acyl: "aspartyl",
        amide: "alpha-asparagine",
        achiral: false,
    },
    Residue {
        symbol: "Cys",
        acid: "cysteine",
        acyl: "cysteinyl",
        amide: "cysteinamide",
        achiral: false,
    },
    Residue {
        symbol: "Gln",
        acid: "glutamine",
        acyl: "glutaminyl",
        amide: "glutaminamide",
        achiral: false,
    },
    Residue {
        symbol: "Glu",
        acid: "glutamic acid",
        acyl: "glutamyl",
        amide: "alpha-glutamine",
        achiral: false,
    },
    Residue {
        symbol: "Gly",
        acid: "glycine",
        acyl: "glycyl",
        amide: "glycinamide",
        achiral: true,
    },
    Residue {
        symbol: "His",
        acid: "histidine",
        acyl: "histidyl",
        amide: "histidinamide",
        achiral: false,
    },
    Residue {
        symbol: "Ile",
        acid: "isoleucine",
        acyl: "isoleucyl",
        amide: "isoleucinamide",
        achiral: false,
    },
    Residue {
        symbol: "Leu",
        acid: "leucine",
        acyl: "leucyl",
        amide: "leucinamide",
        achiral: false,
    },
    Residue {
        symbol: "Lys",
        acid: "lysine",
        acyl: "lysyl",
        amide: "lysinamide",
        achiral: false,
    },
    Residue {
        symbol: "Met",
        acid: "methionine",
        acyl: "methionyl",
        amide: "methioninamide",
        achiral: false,
    },
    Residue {
        symbol: "Phe",
        acid: "phenylalanine",
        acyl: "phenylalanyl",
        amide: "phenylalaninamide",
        achiral: false,
    },
    Residue {
        symbol: "Pro",
        acid: "proline",
        acyl: "prolyl",
        amide: "prolinamide",
        achiral: false,
    },
    Residue {
        symbol: "Ser",
        acid: "serine",
        acyl: "seryl",
        amide: "serinamide",
        achiral: false,
    },
    Residue {
        symbol: "Thr",
        acid: "threonine",
        acyl: "threonyl",
        amide: "threoninamide",
        achiral: false,
    },
    Residue {
        symbol: "Trp",
        acid: "tryptophan",
        acyl: "tryptophyl",
        amide: "tryptophanamide",
        achiral: false,
    },
    Residue {
        symbol: "Tyr",
        acid: "tyrosine",
        acyl: "tyrosyl",
        amide: "tyrosinamide",
        achiral: false,
    },
    Residue {
        symbol: "Val",
        acid: "valine",
        acyl: "valyl",
        amide: "valinamide",
        achiral: false,
    },
    Residue {
        symbol: "Orn",
        acid: "ornithine",
        acyl: "ornithyl",
        amide: "ornithinamide",
        achiral: false,
    },
    Residue {
        symbol: "Cit",
        acid: "citrulline",
        acyl: "citrullyl",
        amide: "citrullinamide",
        achiral: false,
    },
    Residue {
        symbol: "Nle",
        acid: "norleucine",
        acyl: "norleucyl",
        amide: "norleucinamide",
        achiral: false,
    },
    Residue {
        symbol: "Nva",
        acid: "norvaline",
        acyl: "norvalyl",
        amide: "norvalinamide",
        achiral: false,
    },
    Residue {
        symbol: "Hse",
        acid: "homoserine",
        acyl: "homoseryl",
        amide: "homoserinamide",
        achiral: false,
    },
    Residue {
        symbol: "Hcy",
        acid: "homocysteine",
        acyl: "homocysteinyl",
        amide: "homocysteinamide",
        achiral: false,
    },
    Residue {
        symbol: "Hyp",
        acid: "hydroxyproline",
        acyl: "hydroxyprolyl",
        amide: "hydroxyprolinamide",
        achiral: false,
    },
    Residue {
        symbol: "Sar",
        acid: "sarcosine",
        acyl: "sarcosyl",
        amide: "sarcosinamide",
        achiral: true,
    },
    Residue {
        symbol: "Glp",
        acid: "pyroglutamic acid",
        acyl: "pyroglutamyl",
        amide: "pyroglutamide",
        achiral: false,
    },
    // Use an amino-acid stem so preceding acyl groups attach to the backbone
    // N. Generic "2-aminoisobutyric acid" does not retain that default in OPSIN.
    Residue {
        symbol: "Aib",
        acid: "2-methylalanine",
        acyl: "2-methylalanyl",
        amide: "2-methylalaninamide",
        achiral: true,
    },
];

fn residue(symbol: &str) -> Option<&'static Residue> {
    RESIDUES.iter().find(|residue| residue.symbol == symbol)
}

fn n_terminal(symbol: &str) -> Option<&'static str> {
    Some(match symbol {
        "H" => "",
        "Ac" => "N-acetyl-",
        "Boc" => "N-(tert-butoxycarbonyl)-",
        "Fmoc" => "N-[(9H-fluoren-9-yl)methoxycarbonyl]-",
        "Z" | "Cbz" => "N-(benzyloxycarbonyl)-",
        "Tos" => "N-tosyl-",
        "For" => "N-formyl-",
        "PhCO" => "N-benzoyl-",
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Terminus {
    Acid,
    Ester(&'static str),
    Amide,
}

fn c_terminal(symbol: &str) -> Option<Terminus> {
    Some(match symbol {
        "OH" => Terminus::Acid,
        "OMe" => Terminus::Ester("methyl"),
        "OEt" => Terminus::Ester("ethyl"),
        "OBzl" => Terminus::Ester("benzyl"),
        "OBut" => Terminus::Ester("tert-butyl"),
        "NH2" => Terminus::Amide,
        _ => return None,
    })
}

/// Return None unless every symbol belongs to the complete supported grammar.
/// This keeps ordinary chemical names and unsupported extensions on the normal
/// OPSIN path, without rewriting a recognised fragment of an unfamiliar name.
pub(super) fn expand(name: &str) -> Result<Option<Expansion>, Error> {
    let name = name.trim();
    let (cyclic, sequence) = match name.strip_prefix("cyclo(") {
        Some(sequence) => match sequence.strip_suffix(')') {
            Some(sequence) => (true, sequence.trim()),
            None => return Ok(None),
        },
        None => (false, name),
    };
    // IUPAC's cyclo(-Val-Orn-...-) writes the ring-closing bonds explicitly.
    let sequence = if cyclic {
        let sequence = sequence.strip_prefix('-').unwrap_or(sequence);
        sequence.strip_suffix('-').unwrap_or(sequence)
    } else {
        sequence
    };
    let tokens: Vec<_> = sequence.split('-').map(str::trim).collect();
    if tokens.iter().any(|token| token.is_empty()) {
        return Ok(None);
    }
    let mut tokens = tokens.into_iter();
    let n_group = tokens
        .as_slice()
        .first()
        .and_then(|symbol| n_terminal(symbol));
    if n_group.is_some() {
        tokens.next();
    }
    let c_group = tokens
        .as_slice()
        .last()
        .and_then(|symbol| c_terminal(symbol));
    if c_group.is_some() {
        tokens.next_back();
    }
    let mut residues = Vec::new();
    while let Some(token) = tokens.next() {
        let (configuration, symbol) = match token {
            "D" | "L" | "DL" => {
                let Some(symbol) = tokens.next() else {
                    return Ok(None);
                };
                (Some(token), symbol)
            }
            _ => (None, token),
        };
        let Some(amino_acid) = residue(symbol) else {
            return Ok(None);
        };
        residues.push((amino_acid, configuration));
    }
    if residues.is_empty() {
        return Ok(None);
    }
    if cyclic && (n_group.is_some() || c_group.is_some()) {
        return Err(Error::Rejected(
            "A head-to-tail cyclic peptide cannot also have N- or C-terminal groups".into(),
        ));
    }
    if cyclic && residues.len() < 2 {
        return Err(Error::Rejected(
            "A cyclic peptide sequence needs at least two residues".into(),
        ));
    }
    // A strict OPSIN DL parse carries a racemic semantic CXSMILES layer. The
    // editable naming adapter cannot retain that layer, so never substitute a
    // single enantiomer or silently remove its DL designation.
    for (amino_acid, configuration) in &residues {
        if amino_acid.achiral && configuration.is_some() {
            return Err(Error::Rejected(format!(
                "{} is achiral and has no D/L configuration; omit its configuration prefix",
                amino_acid.symbol
            )));
        }
        if *configuration == Some("DL") {
            return Err(Error::Rejected(
                "DL peptide residues require racemic or relative stereochemistry, which editable naming previews cannot retain; no single enantiomer was substituted".into(),
            ));
        }
    }
    let mut expanded = String::new();
    if cyclic {
        expanded.push_str("cyclo(");
    } else if let Some(n_group) = n_group {
        expanded.push_str(n_group);
    }
    for (index, (amino_acid, configuration)) in residues.iter().enumerate() {
        if index > 0 {
            expanded.push('-');
        }
        if !amino_acid.achiral {
            expanded.push_str(configuration.unwrap_or("L"));
            expanded.push('-');
        }
        if cyclic || index + 1 < residues.len() {
            expanded.push_str(amino_acid.acyl);
        } else if c_group == Some(Terminus::Amide) {
            expanded.push_str(amino_acid.amide);
        } else {
            expanded.push_str(amino_acid.acid);
        }
    }
    if cyclic {
        expanded.push(')');
    } else if let Some(Terminus::Ester(alkyl)) = c_group {
        // On dicarboxylic residues C-1 is the peptide C terminus; their side
        // chains stay acids. State the ester locant rather than relying on an
        // unlocanted dicarboxylic-acid ester interpretation.
        if residues
            .last()
            .is_some_and(|(residue, _)| matches!(residue.symbol, "Asp" | "Glu"))
        {
            expanded.push_str(" 1-");
        } else {
            expanded.push(' ');
        }
        expanded.push_str(alkyl);
        expanded.push_str(" ester");
    }
    let defaulted = residues
        .iter()
        .any(|(residue, configuration)| !residue.achiral && configuration.is_none());
    Ok(Some(Expansion {
        name: expanded,
        warnings: defaulted
            .then(|| "Residues without a D- or L- prefix read as L (IUPAC 3AA-14.5)".into())
            .into_iter()
            .collect(),
    }))
}

/// The explicit one-letter mode accepts contiguous symbols or single symbols
/// separated by spaces. It never runs automatically on ordinary name input.
pub(super) fn one_letter(name: &str) -> Result<String, Error> {
    let mut sequence = Vec::new();
    for symbol in name.chars().filter(|symbol| !symbol.is_whitespace()) {
        let residue = match symbol.to_ascii_uppercase() {
            'A' => "Ala",
            'R' => "Arg",
            'N' => "Asn",
            'D' => "Asp",
            'C' => "Cys",
            'Q' => "Gln",
            'E' => "Glu",
            'G' => "Gly",
            'H' => "His",
            'I' => "Ile",
            'L' => "Leu",
            'K' => "Lys",
            'M' => "Met",
            'F' => "Phe",
            'P' => "Pro",
            'S' => "Ser",
            'T' => "Thr",
            'W' => "Trp",
            'Y' => "Tyr",
            'V' => "Val",
            _ => {
                return Err(Error::Rejected(format!(
                    "Unknown or ambiguous one-letter peptide symbol '{symbol}'; use the 20 standard amino-acid symbols"
                )));
            }
        };
        sequence.push(residue);
    }
    if sequence.is_empty() {
        return Err(Error::Rejected(
            "Enter a one-letter peptide sequence".into(),
        ));
    }
    Ok(sequence.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expanded(input: &str) -> String {
        expand(input).unwrap().unwrap().name
    }

    fn assert_graph(input: &str, expected: &str) {
        let name = expanded(input);
        // Exercise the same strict semantic adapter on native and browser
        // paths, including warning, connectedness, and CXSMILES checks.
        let request = crate::naming::local::parse_request(&name).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let record = crate::naming::local::parse_result(&name, &reply)
            .unwrap_or_else(|error| panic!("{input} -> {name}: {error}"));
        crate::naming::verify_identity(expected, &record.smiles)
            .unwrap_or_else(|error| panic!("{input} -> {name}: {error}; {}", record.smiles));
    }

    #[test]
    fn complete_symbols_expand_and_unfamiliar_names_pass_unchanged() {
        assert_eq!(
            expanded("Boc-Ala-D-Phe-Gly-OMe"),
            "N-(tert-butoxycarbonyl)-L-alanyl-D-phenylalanyl-glycine methyl ester"
        );
        assert_eq!(
            expanded("Glp-His-Pro-NH2"),
            "L-pyroglutamyl-L-histidyl-L-prolinamide"
        );
        assert_eq!(
            expanded("cyclo(-Val-Orn-Leu-D-Phe-Pro-)"),
            "cyclo(L-valyl-L-ornithyl-L-leucyl-D-phenylalanyl-L-prolyl)"
        );
        assert_eq!(expanded("Gly-OH"), "glycine");
        assert_eq!(expanded("H-Ala-Gly-OH"), "L-alanyl-glycine");
        assert_eq!(expanded("L-Ala-D-Phe"), "L-alanyl-D-phenylalanine");
        for input in [
            "ethanol",
            "AG",
            "A",
            "ALA-GLY",
            "alanyl-glycine",
            "Ala-unknown",
            "Ala-Gly-unknown",
            "Lys(Boc)-Gly",
            "Glu(OtBu)-Gly",
            "Ala--Gly",
            "Ala-",
            "cyclo(Ala-Gly",
            "Asx-Gly",
            "Xaa-Gly",
            "DL-unknown",
        ] {
            assert!(expand(input).unwrap().is_none(), "{input}");
        }
        for input in ["DL-Ala-Gly", "Gly-DL-Phe"] {
            let error = expand(input).err().unwrap().to_string();
            assert!(error.contains("racemic"), "{input}: {error}");
        }
        for input in [
            "D-Gly-Ala",
            "L-Sar-Gly",
            "DL-Gly",
            "D-Aib",
            "L-Aib",
            "DL-Aib",
        ] {
            assert!(
                expand(input).err().unwrap().to_string().contains("achiral"),
                "{input}"
            );
        }
        for input in [
            "cyclo(Ala)",
            "cyclo(Boc-Ala-Gly)",
            "cyclo(Ala-Gly-OMe)",
            "cyclo(H-Ala-Gly)",
        ] {
            assert!(expand(input).is_err(), "{input}");
        }
        assert!(super::super::expand(&vec!["Trp"; 200].join("-")).is_err());
    }

    #[test]
    fn one_letter_is_explicit_accepts_spaces_and_names_bad_symbols() {
        assert_eq!(one_letter("AG").unwrap(), "Ala-Gly");
        assert_eq!(one_letter("A G").unwrap(), "Ala-Gly");
        assert_eq!(one_letter("  a g  ").unwrap(), "Ala-Gly");
        assert_eq!(
            one_letter("ARNDCQEGHILKMFPSTWYV").unwrap(),
            "Ala-Arg-Asn-Asp-Cys-Gln-Glu-Gly-His-Ile-Leu-Lys-Met-Phe-Pro-Ser-Thr-Trp-Tyr-Val"
        );
        for symbol in ['B', 'Z', 'X', 'J', 'O', 'U', '-', 'α'] {
            let error = one_letter(&format!("A{symbol}G")).unwrap_err().to_string();
            assert!(error.contains(&format!("'{symbol}'")), "{error}");
        }
        for input in ["", " "] {
            assert!(one_letter(input).is_err(), "{input:?}");
        }
        assert!(expand("AG").unwrap().is_none());
    }

    #[test]
    fn every_residue_acid_acyl_and_amide_matches_an_independent_backbone() {
        // Independently specified residue graphs. Each neutral N terminus is
        // on the left; the final carbonyl is the backbone C terminus. These
        // encode the natural L configurations, including both Ile/Thr/Hyp
        // centres and L-Cys's opposite CIP letter from L-Ala.
        for (symbol, backbone) in [
            ("Ala", "N[C@@H](C)C(=O)"),
            ("Arg", "N[C@@H](CCCNC(=N)N)C(=O)"),
            ("Asn", "N[C@@H](CC(=O)N)C(=O)"),
            ("Asp", "N[C@@H](CC(=O)O)C(=O)"),
            ("Cys", "N[C@@H](CS)C(=O)"),
            ("Gln", "N[C@@H](CCC(=O)N)C(=O)"),
            ("Glu", "N[C@@H](CCC(=O)O)C(=O)"),
            ("Gly", "NCC(=O)"),
            ("His", "N[C@@H](Cc1c[nH]cn1)C(=O)"),
            ("Ile", "N[C@@H]([C@@H](C)CC)C(=O)"),
            ("Leu", "N[C@@H](CC(C)C)C(=O)"),
            ("Lys", "N[C@@H](CCCCN)C(=O)"),
            ("Met", "N[C@@H](CCSC)C(=O)"),
            ("Phe", "N[C@@H](Cc1ccccc1)C(=O)"),
            ("Pro", "N1[C@@H](CCC1)C(=O)"),
            ("Ser", "N[C@@H](CO)C(=O)"),
            ("Thr", "N[C@@H]([C@H](O)C)C(=O)"),
            ("Trp", "N[C@@H](Cc1c[nH]c2ccccc12)C(=O)"),
            ("Tyr", "N[C@@H](Cc1ccc(O)cc1)C(=O)"),
            ("Val", "N[C@@H](C(C)C)C(=O)"),
            ("Orn", "N[C@@H](CCCN)C(=O)"),
            ("Cit", "N[C@@H](CCCNC(=O)N)C(=O)"),
            ("Nle", "N[C@@H](CCCC)C(=O)"),
            ("Nva", "N[C@@H](CCC)C(=O)"),
            ("Hse", "N[C@@H](CCO)C(=O)"),
            ("Hcy", "N[C@@H](CCS)C(=O)"),
            ("Hyp", "N1[C@@H](C[C@@H](O)C1)C(=O)"),
            ("Sar", "N(C)CC(=O)"),
            ("Glp", "N1[C@@H](CCC1=O)C(=O)"),
            ("Aib", "NC(C)(C)C(=O)"),
        ] {
            assert_graph(symbol, &format!("{backbone}O"));
            assert_graph(&format!("{symbol}-Gly"), &format!("{backbone}NCC(=O)O"));
            assert_graph(&format!("Gly-{symbol}-NH2"), &format!("NCC(=O){backbone}N"));
        }
    }

    #[test]
    fn termini_and_cyclic_peptides_match_independent_graphs() {
        assert_graph("H-Ala-Gly-OH", "N[C@@H](C)C(=O)NCC(=O)O");
        for (group, prefix) in [
            ("Ac", "CC(=O)"),
            ("Boc", "CC(C)(C)OC(=O)"),
            ("Fmoc", "O=C(OCC1c2ccccc2-c2ccccc21)"),
            ("Z", "O=C(OCc1ccccc1)"),
            ("Cbz", "O=C(OCc1ccccc1)"),
            ("Tos", "Cc1ccc(S(=O)(=O)"),
            ("For", "O=C"),
            ("PhCO", "O=C(c1ccccc1)"),
        ] {
            let expected = if group == "Tos" {
                format!("{prefix}N[C@@H](C)C(=O)NCC(=O)O)cc1")
            } else {
                format!("{prefix}N[C@@H](C)C(=O)NCC(=O)O")
            };
            assert_graph(&format!("{group}-Ala-Gly"), &expected);
        }
        for (group, ending) in [
            ("OH", "O"),
            ("OMe", "OC"),
            ("OEt", "OCC"),
            ("OBzl", "OCc1ccccc1"),
            ("OBut", "OC(C)(C)C"),
            ("NH2", "N"),
        ] {
            assert_graph(
                &format!("Ala-Gly-{group}"),
                &format!("N[C@@H](C)C(=O)NCC(=O){ending}"),
            );
            assert_graph(
                &format!("Gly-Asp-{group}"),
                &format!("NCC(=O)N[C@@H](CC(=O)O)C(=O){ending}"),
            );
            assert_graph(
                &format!("Gly-Glu-{group}"),
                &format!("NCC(=O)N[C@@H](CCC(=O)O)C(=O){ending}"),
            );
        }
        assert_graph(
            "Boc-Ala-D-Phe-Gly-OMe",
            "CC(C)(C)OC(=O)N[C@@H](C)C(=O)N[C@H](Cc1ccccc1)C(=O)NCC(=O)OC",
        );
        assert_graph(
            "Tos-Arg-OMe",
            "Cc1ccc(S(=O)(=O)N[C@@H](CCCNC(=N)N)C(=O)OC)cc1",
        );
        assert_graph(
            "Glp-His-Pro-NH2",
            "O=C1CC[C@@H](C(=O)N[C@@H](Cc2c[nH]cn2)C(=O)N2CCC[C@H]2C(=O)N)N1",
        );
        assert_graph("cyclo(-Ala-Gly-)", "O=C1NCC(=O)N[C@H]1C");
        assert_graph(
            "cyclo(-Val-Orn-Leu-D-Phe-Pro-)",
            "N1[C@@H](C(C)C)C(=O)N[C@@H](CCCN)C(=O)N[C@@H](CC(C)C)C(=O)N[C@H](Cc2ccccc2)C(=O)N2CCC[C@H]2C1=O",
        );
        assert_graph(
            "D-Ile-D-Thr-Gly",
            "N[C@H]([C@H](C)CC)C(=O)N[C@H]([C@@H](O)C)C(=O)NCC(=O)O",
        );
        assert_graph("Gly-Aib-Gly", "NCC(=O)NC(C)(C)C(=O)NCC(=O)O");
        assert_graph("Aib-Aib", "NC(C)(C)C(=O)NC(C)(C)C(=O)O");
        assert_graph("cyclo(-Aib-Gly-)", "O=C1NCC(=O)NC1(C)C");
        assert_graph(
            "Boc-Ala-Aib-D-Phe-OMe",
            "CC(C)(C)OC(=O)N[C@@H](C)C(=O)NC(C)(C)C(=O)N[C@H](Cc1ccccc1)C(=O)OC",
        );
    }
}
