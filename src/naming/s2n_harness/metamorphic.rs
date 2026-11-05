//! Metamorphic relations: equivalent inputs must get the same name and status.
//! Failures listed in `metamorphic_known_failures.tsv` are open bugs.
use super::{FIXTURES, Measurements, corpus, generator, namer};
use crate::{
    chemistry::{
        smiles::write::{Options, write},
        stereo::perception::State,
    },
    naming::{NameStatus, canonical_smiles},
};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
    sync::LazyLock,
};

type Outcome = Result<(String, NameStatus), ()>;

/// Gaps in the fast subset: isotopes with an equivalent partner, salts and
/// ions, meso and pseudoasymmetric centres, E/Z and a biaryl axis.
const CURATED: &[&str] = &[
    "[13CH3]C",
    "[13CH3]CC",
    "[13cH]1ccccc1",
    "[15NH2]CN",
    "[15NH2]CCN",
    "[15n]1ccncc1",
    "[2H]OC",
    "[Na+].[Cl-]",
    "CC(=O)[O-].[Na+]",
    "C[NH3+].[Cl-]",
    "[Ca+2].[Cl-].[Cl-]",
    "[NH4+].[NH4+].[O-]S(=O)(=O)[O-]",
    "C[N+](C)(C)C.[Br-]",
    "[NH3+]CC(=O)[O-]",
    "C[N+](C)(C)CC(=O)[O-]",
    "C[NH2+]C",
    "[O-][N+](=O)c1ccccc1",
    "O=C(O)[C@H](O)[C@H](O)C(=O)O",
    "C[C@H](O)[C@@H](O)C",
    "C[C@H](O)[C@H](O)C",
    "C[C@H](F)C([C@H](F)C)CCCC",
    "C[C@H]1CC[C@@H](C)CC1",
    "C[C@H]1C[C@H](C)C1",
    "C[C@H](O)[C@H](Cl)[C@H](O)C",
    "C[C@H](O)[C@@H](Cl)[C@H](O)C",
    // Only the R-before-S rule decides the numbering of the enantiomers.
    "C[C@H](O)[C@@H](O)[C@H](O)[C@H](O)C",
    "C/C=C/C",
    "C/C=C\\C",
    "C/C=C/[C@H](F)Cl",
    // Naming SMILES of an M biaryl axis (map 900004; P is 900008).
    "C[C@H](O)c1cccc[c:900004]1-[c:900004]1ccccc1O",
];

/// Every distinct canonical fast-subset and curated structure with its name.
static BASE: LazyLock<Vec<(String, Outcome)>> = LazyLock::new(|| {
    let mut seen = HashSet::new();
    let curated = CURATED
        .iter()
        .map(|smiles| canonical_smiles(smiles).unwrap_or_else(|error| panic!("{smiles}: {error}")));
    corpus(true)
        .into_iter()
        .map(|case| case.smiles)
        .chain(
            generator::generate(500)
                .into_iter()
                .map(|(smiles, _)| smiles),
        )
        .filter_map(|smiles| canonical_smiles(&smiles).ok())
        .chain(curated)
        .filter(|c| seen.insert(c.clone()))
        .map(|c| {
            let base = outcome(&c);
            (c, base)
        })
        .collect()
});

fn outcome(smiles: &str) -> Outcome {
    namer(smiles)
        .map(|name| (name.text, name.status))
        .map_err(drop)
}

fn describe(outcome: &Outcome) -> String {
    match outcome {
        Ok((text, status)) => format!("{text} [{status:?}]"),
        Err(()) => "declined".into(),
    }
}

fn state(smiles: &str) -> State {
    crate::chemistry::smiles::prepare(smiles).unwrap().state
}

fn smiles_in_order(state: &State, root: Option<usize>) -> String {
    let options = Options {
        canonical: false,
        root,
        ..Default::default()
    };
    write(state, options).unwrap().text
}

/// Run `relation` on every structure; it returns one line per mismatch. The
/// test fails on failures missing from the known-failures file and on stale
/// entries there.
fn check(name: &str, mut relation: impl FnMut(&str, &Outcome) -> Vec<String>) {
    let text = std::fs::read_to_string(Path::new(FIXTURES).join("metamorphic_known_failures.tsv"))
        .expect("read metamorphic known failures");
    let mut known: BTreeMap<_, _> = text
        .lines()
        .skip(1)
        .map(|line| line.splitn(3, '\t').collect::<Vec<_>>())
        .inspect(|fields| {
            assert!(
                ["atom_order", "mirror", "atom_maps", "isotope_swap"].contains(&fields[0]),
                "unknown relation in known failures: {fields:?}"
            );
        })
        .filter(|fields| fields[0] == name)
        .map(|fields| (fields[1], fields[2]))
        .collect();
    let mut new = Vec::new();
    for (c, base) in BASE.iter() {
        let mismatches = relation(c, base);
        if mismatches.is_empty() {
            continue;
        }
        let label = if known.remove(c.as_str()).is_some() {
            "known"
        } else {
            new.push(c.as_str());
            "NEW"
        };
        for mismatch in mismatches {
            println!("{label} {name} {c}: {mismatch}");
        }
    }
    println!(
        "{name}: {} structures, {} new failures",
        BASE.len(),
        new.len()
    );
    assert!(
        new.is_empty() && known.is_empty(),
        "{name}: new failures {new:#?}; stale known failures (now pass, remove them) {known:#?}"
    );
}

fn mismatch(detail: &str, expected: &Outcome, actual: &Outcome) -> Option<String> {
    (expected != actual).then(|| {
        format!(
            "{detail}: expected {}, got {}",
            describe(expected),
            describe(actual)
        )
    })
}

#[test]
fn atom_order_keeps_name_and_status() {
    check("atom_order", |c, base| {
        // FNV-1a: a stable per-structure seed, so new rows don't reshuffle others.
        let seed = c.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
        });
        (0..3)
            .filter_map(|order| {
                let mut rng = generator::SplitMix64(seed.wrapping_add(order));
                let mut permuted = c.to_owned();
                // Each re-read renumbers atoms in the previous traversal order.
                for _ in 0..3 {
                    let state = state(&permuted);
                    let root = rng.below(state.graph.atoms.len());
                    permuted = smiles_in_order(&state, Some(root));
                }
                // The writer keeps component order, so shuffle the components too.
                let mut components: Vec<_> = permuted.split('.').collect();
                for i in (1..components.len()).rev() {
                    components.swap(i, rng.below(i + 1));
                }
                let permuted = components.join(".");
                match canonical_smiles(&permuted) {
                    Ok(identity) if identity == c => mismatch(&permuted, base, &outcome(&permuted)),
                    identity => Some(format!("{permuted} has identity {identity:?}")),
                }
            })
            .collect()
    });
}

/// `text` with every CIP descriptor in a descriptor group mapped by `map`.
/// Groups are innermost parentheses before a hyphen whose items are all
/// `locant? descriptor`, as `(2R,4aS,1'M,NS)-`.
fn map_descriptors(text: &str, map: impl Fn(&str) -> &str) -> String {
    const DESCRIPTORS: [&str; 10] = ["Ra", "Sa", "R", "S", "M", "P", "r", "s", "E", "Z"];
    let is_locant = |locant: &str| {
        let bare = locant.trim_end_matches(['\'', '′']);
        let letter = bare.trim_start_matches(|c: char| c.is_ascii_digit());
        locant.is_empty()
            || matches!(bare, "N" | "P" | "S")
            || (letter.len() < bare.len()
                && (letter.is_empty() || matches!(letter.as_bytes(), [b'a'..=b'z'])))
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        out.push_str(&rest[..=open]);
        rest = &rest[open + 1..];
        let Some(close) = rest
            .find(['(', ')'])
            .filter(|&i| rest[i..].starts_with(")-"))
        else {
            continue;
        };
        let items: Option<Vec<_>> = rest[..close]
            .split(',')
            .map(|item| {
                let descriptor = DESCRIPTORS.into_iter().find(|d| item.ends_with(d))?;
                let locant = &item[..item.len() - descriptor.len()];
                is_locant(locant).then(|| format!("{locant}{}", map(descriptor)))
            })
            .collect();
        if let Some(items) = items {
            out.push_str(&items.join(","));
            rest = &rest[close..];
        }
    }
    out + rest
}

fn enantiomer_descriptor(descriptor: &str) -> &str {
    match descriptor {
        "R" => "S",
        "S" => "R",
        "M" => "P",
        "P" => "M",
        "Ra" => "Sa",
        "Sa" => "Ra",
        other => other,
    }
}

#[test]
fn descriptor_groups_are_mirrored_and_nothing_else() {
    for (text, expected) in [
        (
            "(2R,4aS,1'M,2''P,NR,3Ra,4Sa,5r,6s,7E,8Z)-x",
            "(2S,4aR,1'P,2''M,NS,3Sa,4Ra,5r,6s,7E,8Z)-x",
        ),
        ("(2R)-1-[(S)-x]y", "(2S)-1-[(R)-x]y"),
        (
            "S-methyl (2E)-N,N-dimethyl(31P)phosphane (RS)-y (S-methyl)-z",
            "S-methyl (2E)-N,N-dimethyl(31P)phosphane (RS)-y (S-methyl)-z",
        ),
    ] {
        assert_eq!(map_descriptors(text, enantiomer_descriptor), expected);
    }
}

/// The mirror image (every `@`/`@@` and M/P axis marker swapped) is named
/// with every R/S, M/P and Ra/Sa swapped, r/s and E/Z kept, and the same
/// status. Achiral and meso inputs are their own mirror image. When only the
/// R-before-S rule fixes the numbering (P-14.4(j)) or parent (P-44.4.1.12.2),
/// the enantiomer may use the other orientation or arm: the name may then
/// differ in descriptors only, and mirroring it back must name the input.
#[test]
fn mirror_image_swaps_only_descriptors() {
    let mut parser = Measurements::default();
    check("mirror", |c, base| {
        let mirrored = c
            .replace("@@", "\u{1}")
            .replace('@', "@@")
            .replace('\u{1}', "@")
            .replace(":900004]", ":\u{1}]")
            .replace(":900008]", ":900004]")
            .replace(":\u{1}]", ":900008]");
        let mirror = match canonical_smiles(&mirrored) {
            Ok(mirror) => mirror,
            Err(error) => return vec![format!("{mirrored}: {error}")],
        };
        let actual = outcome(&mirrored);
        if mirror == c {
            return mismatch(&mirrored, base, &actual).into_iter().collect();
        }
        if let (Ok((text, status)), Ok((base_text, base_status))) = (&actual, base) {
            let unnumbered = |text: &str| map_descriptors(text, |_| "");
            if status == base_status
                && unnumbered(text) == unnumbered(base_text)
                && parser
                    .parse_name(&map_descriptors(text, enantiomer_descriptor))
                    .as_deref()
                    == Ok(c)
            {
                return vec![];
            }
        }
        let expected = base
            .clone()
            .map(|(text, status)| (map_descriptors(&text, enantiomer_descriptor), status));
        mismatch(&mirrored, &expected, &actual)
            .into_iter()
            .collect()
    });
}

#[test]
fn atom_maps_keep_name_and_status() {
    check("atom_maps", |c, base| {
        let mut state = state(c);
        // Axis markers are atom maps with a meaning of their own.
        if state.metadata.atoms.iter().any(|atom| atom.map_present) {
            return vec![];
        }
        for (map, atom) in (1..).zip(&mut state.metadata.atoms) {
            atom.map_number = map;
            atom.map_present = true;
        }
        let mapped = smiles_in_order(&state, None);
        mismatch(&mapped, base, &outcome(&mapped))
            .into_iter()
            .collect()
    });
}

/// Moving an isotope label to an equivalent atom (same canonical structure)
/// keeps the name and status.
#[test]
fn isotope_swaps_keep_name_and_status() {
    let mut swaps = 0;
    check("isotope_swap", |c, base| {
        let state = state(c);
        let atoms = &state.graph.atoms;
        let mut mismatches = Vec::new();
        for (i, labelled) in atoms.iter().enumerate().filter(|(_, a)| a.isotope != 0) {
            for (j, _) in atoms
                .iter()
                .enumerate()
                .filter(|(_, a)| a.isotope == 0 && a.atomic_number == labelled.atomic_number)
            {
                let mut swapped = state.clone();
                swapped.graph.atoms[i].isotope = 0;
                swapped.graph.atoms[j].isotope = labelled.isotope;
                let swapped = smiles_in_order(&swapped, None);
                if canonical_smiles(&swapped).as_deref() == Ok(c) {
                    swaps += 1;
                    mismatches.extend(mismatch(&swapped, base, &outcome(&swapped)));
                }
            }
        }
        mismatches
    });
    assert!(swaps > 0, "no equivalent isotope positions were tested");
}
