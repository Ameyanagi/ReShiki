//! Reproducible graph-first inputs, independent of naming implementation rules.

use std::collections::HashSet;

const FAMILIES: &[&str] = &[
    "alkyl",
    "alkene",
    "alkyne",
    "monocycle",
    "heteroaromatic",
    "ring-assembly",
    "spiro",
    "bicyclo",
    "tricyclo",
    "fused-aromatic",
    "acid",
    "ester",
    "amide",
    "n-substituted-amide",
    "nitrile",
    "aldehyde",
    "ketone",
    "alcohol",
    "amine",
    "halide",
    "ether",
    "alkene-stereo",
    "tetrahedral-stereo",
    "ring-with-group",
];

/// Ring scaffolds of families 3–9, monocycle through fused-aromatic.
const RINGS: &[&[&str]] = &[
    &[
        "C1CC1",
        "C1CCC1",
        "C1CCCC1",
        "C1CCCCC1",
        "C1CCCCCC1",
        "C1CCCCCCC1",
    ],
    &[
        "c1ccncc1",
        "c1cc[nH]c1",
        "c1ccoc1",
        "c1ccsc1",
        "c1ncc[nH]1",
        "c1ncco1",
        "c1nccs1",
        "c1cncnc1",
    ],
    &[
        "c1ccccc1-c2ccccc2",
        "C1CCCCC1-C2CCCCC2",
        "c1ccccc1-c2ccncc2",
        "C1CCC1-C2CCCC2",
        "c1ccoc1-c2ccsc2",
    ],
    &[
        "C1CC2(C1)CC2",
        "C1CC2(C1)CCC2",
        "C1CCC2(CC1)CCC2",
        "C1CCC2(CC1)CCCC2",
    ],
    &["C1CC2CCC1C2", "C1CCC2CCC1C2", "C1CC2CCCC1C2", "C1C2CC1CC2"],
    &[
        "C1C2CC3CC1CC(C2)C3",
        "C1CC2CCC3CCC1C23",
        "C1C2CC3CC1CC2C3",
        "C1CC2C3CCC1C2C3",
    ],
    &[
        "c1ccc2ccccc2c1",
        "c1ccc2[nH]ccc2c1",
        "c1ccc2ncccc2c1",
        "c1ccc2occc2c1",
    ],
];
/// Groups written so their last atom bonds to the ring's first atom.
const GROUPS: &[&str] = &[
    "OC(=O)",
    "COC(=O)",
    "NC(=O)",
    "CNC(=O)",
    "CN(C)C(=O)",
    "N#C",
    "O=C",
    "CC(=O)",
    "O",
    "N",
    "CN",
    "F",
    "Cl",
    "Br",
    "CO",
];

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// Both modes consume this same stream: `generate(500)` is an exact prefix of
/// `generate(3000)`. Each accepted input advances to the next family, so a
/// duplicate cannot starve a family out of the fast corpus.
pub(super) fn generate(count: usize) -> Vec<(String, String)> {
    let mut rng = SplitMix64(0x7353_326e_7068_6173);
    let mut serials = [0; FAMILIES.len()];
    let mut seen = HashSet::new();
    let mut generated = Vec::with_capacity(count);
    let mut attempts = 0usize;
    let limit = count.saturating_mul(128).max(4096);
    for index in 0..count {
        let family = index % FAMILIES.len();
        loop {
            attempts += 1;
            assert!(attempts <= limit, "s2n generator exhausted its attempt cap");
            let serial = serials[family];
            serials[family] += 1;
            let smiles = candidate(family, serial, &mut rng);
            // Validate every attempted graph, including duplicates. A broken
            // template must fail loudly instead of quietly shrinking coverage.
            let canonical = crate::naming::canonical_smiles(&smiles).unwrap_or_else(|error| {
                panic!(
                    "invalid generated {} candidate {smiles:?}: {error}",
                    FAMILIES[family]
                )
            });
            if seen.insert(canonical) {
                generated.push((smiles, FAMILIES[family].to_owned()));
                break;
            }
        }
    }
    generated
}

/// A rooted, saturated carbon fragment. Branching only at internal backbone
/// atoms makes concatenation safe at either end and supplies many small graphs.
fn alkyl(rng: &mut SplitMix64) -> String {
    let length = 2 + rng.below(7);
    let mut result = String::from("C");
    for position in 1..length {
        result.push('C');
        if position + 1 < length {
            match rng.below(4) {
                1 => result.push_str("(C)"),
                2 => result.push_str("(CC)"),
                _ => {}
            }
        }
    }
    result
}

fn scaffold(cores: &[&str], serial: usize, rng: &mut SplitMix64) -> String {
    let core = cores[serial % cores.len()];
    if serial < cores.len() {
        core.to_owned()
    } else {
        format!("{}{core}", alkyl(rng))
    }
}

fn candidate(family: usize, serial: usize, rng: &mut SplitMix64) -> String {
    // Emit the small defining examples first; decoration starts only after
    // every scaffold in a family has appeared in the shared stream.
    if family == 23 {
        // A ring scaffold, an optional linker carrying R/S or E/Z, then a group;
        // written so the scaffold's first atom carries the substituent.
        let rings = RINGS[rng.below(RINGS.len())];
        let ring = rings[rng.below(rings.len())];
        let group = GROUPS[rng.below(GROUPS.len())];
        let linker = match rng.below(5) {
            0 => "",
            1 => "C",
            2 => "CC",
            3 => ["[C@H](C)", "[C@@H](C)"][rng.below(2)],
            _ => ["/C=C/", "/C=C\\"][rng.below(2)],
        };
        return format!("{group}{linker}{ring}");
    }
    if (3..=9).contains(&family) {
        return scaffold(RINGS[family - 3], serial, rng);
    }
    let bases: &[&str] = match family {
        0 => &[
            "C",
            "CC",
            "CCC",
            "CC(C)C",
            "CC(C)(C)C",
            "CCC(C)C",
            "CC(C)CC(C)C",
        ],
        1 => &["C=C", "CC=C", "CC=CC", "C=CC=C"],
        2 => &["C#C", "CC#C", "CC#CC", "C#CC#C"],
        10 => &["C(=O)O", "CC(=O)O", "CCC(=O)O", "CC(C)C(=O)O"],
        11 => &["CC(=O)OC", "CC(=O)OCC", "CCC(=O)OC"],
        12 => &["CC(=O)N", "CCC(=O)N", "CC(C)C(=O)N"],
        13 => &["CC(=O)NC", "CC(=O)N(C)C", "CCC(=O)NCC"],
        14 => &["CC#N", "CCC#N", "CC(C)C#N"],
        15 => &["C=O", "CC=O", "CCC=O", "CC(C)C=O"],
        16 => &["CC(=O)C", "CCC(=O)C", "CCC(=O)CC"],
        17 => &["CO", "CCO", "CC(O)C", "OCCO"],
        18 => &["CN", "CCN", "CNC", "CN(C)C"],
        19 => &["CF", "CCl", "CBr", "CI"],
        20 => &["COC", "COCC", "CCOCC"],
        21 => &["C/C=C/C", "C/C=C\\C"],
        22 => &["C[C@H](O)C(=O)O", "C[C@@H](O)C(=O)O"],
        _ => unreachable!(),
    };
    if serial < bases.len() {
        return bases[serial].to_owned();
    }
    let left = alkyl(rng);
    match family {
        0 => left,
        1 => format!("{left}C=C{}", alkyl(rng)),
        2 => format!("{left}C#C{}", alkyl(rng)),
        10 => format!("{left}C(=O)O"),
        11 => format!("{left}C(=O)O{}", alkyl(rng)),
        12 => format!("{left}C(=O)N"),
        13 if serial.is_multiple_of(2) => format!("{left}C(=O)N({}){}", alkyl(rng), alkyl(rng)),
        13 => format!("{left}C(=O)N{}", alkyl(rng)),
        14 => format!("{left}C#N"),
        15 => format!("{left}C=O"),
        16 => format!("{left}C(=O){}", alkyl(rng)),
        17 if serial.is_multiple_of(2) => format!("{left}C(O){}", alkyl(rng)),
        17 => format!("{left}O"),
        18 if serial.is_multiple_of(3) => format!("{left}N({}){}", alkyl(rng), alkyl(rng)),
        18 if serial % 3 == 1 => format!("{left}N{}", alkyl(rng)),
        18 => format!("{left}N"),
        19 => format!("{left}{}", ["F", "Cl", "Br", "I"][serial % 4]),
        20 => format!("{left}O{}", alkyl(rng)),
        21 => format!(
            "{left}/C=C{}{}",
            if serial.is_multiple_of(2) { "/" } else { "\\" },
            alkyl(rng)
        ),
        // The acid side can never equal the saturated alkyl side: every
        // explicit center has four distinct ligands, so import retains it.
        22 => format!(
            "{left}[C{}H](O)C(=O)O",
            if serial.is_multiple_of(2) { "@" } else { "@@" }
        ),
        _ => unreachable!(),
    }
}
