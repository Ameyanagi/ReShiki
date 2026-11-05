//! Shared P-14.2 numerical terms (counts through 1000).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Multiplier {
    pub count: usize,
    pub basic: String,
    pub complex: String,
}
const UNITS: [&str; 10] = [
    "", "hen", "do", "tri", "tetra", "penta", "hexa", "hepta", "octa", "nona",
];
const TENS: [&str; 10] = [
    "",
    "deca",
    "icosa",
    "triaconta",
    "tetraconta",
    "pentaconta",
    "hexaconta",
    "heptaconta",
    "octaconta",
    "nonaconta",
];
const HUNDREDS: [&str; 10] = [
    "", "hecta", "dicta", "tricta", "tetracta", "pentacta", "hexacta", "heptacta", "octacta",
    "nonacta",
];
pub fn numerical_term(count: usize) -> String {
    assert!((1..=1000).contains(&count));
    if count == 1000 {
        return "kilia".into();
    }
    let remainder = count % 100;
    let under = if remainder == 11 {
        "undeca".into()
    } else {
        let u = UNITS[remainder % 10];
        let t = TENS[remainder / 10];
        let t = if !u.is_empty() && remainder / 10 == 2 && u.ends_with(['a', 'e', 'i', 'o', 'u']) {
            &t[1..]
        } else {
            t
        };
        format!("{u}{t}")
    };
    format!("{under}{}", HUNDREDS[count / 100])
}
pub fn get(count: usize) -> Multiplier {
    assert!((2..=1000).contains(&count));
    let basic = match count {
        2 => "di".into(),
        3 => "tri".into(),
        _ => numerical_term(count),
    };
    let complex = match count {
        2 => "bis".into(),
        3 => "tris".into(),
        _ => format!("{}akis", basic.trim_end_matches('a')),
    };
    Multiplier {
        count,
        basic,
        complex,
    }
}
pub fn basic(count: usize) -> String {
    get(count).basic
}
/// Render a multiplied suffix with the elision required by P-16.7.1(c).
/// Only suffixes beginning with `a` or `o` elide a numerical prefix's terminal
/// `a`; substituent prefixes and other suffix initials keep it.
/// https://iupac.qmul.ac.uk/BlueBook/P1.html#160701
pub fn suffix(count: usize, suffix: &str) -> String {
    if count <= 1 {
        return suffix.into();
    }
    // P-72.2.2.2.2: these compound anionic suffixes take bis/tris, etc.
    if matches!(suffix, "olate" | "thiolate" | "selenolate" | "tellurolate") {
        return format!("{}({suffix})", complex_(count));
    }
    let mut multiplier = basic(count);
    if suffix.starts_with(['a', 'o']) && multiplier.ends_with('a') {
        multiplier.pop();
    }
    format!("{multiplier}{suffix}")
}
pub fn complex_(count: usize) -> String {
    get(count).complex
}
pub fn candidate_splits(name: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for n in 2..=1000 {
        let m = get(n);
        for p in [m.basic, m.complex] {
            if let Some(rest) = name.strip_prefix(&p) {
                found.push((p.len(), n, rest.to_owned()));
            }
        }
    }
    found.sort_by_key(|(len, _, _)| std::cmp::Reverse(*len));
    found.into_iter().map(|(_, n, s)| (n, s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_multiplier_elides_a_only_before_a_or_o() {
        for (ending, expected) in [
            ("amine", "tetramine"),
            ("one", "tetrone"),
            ("ol", "tetrol"),
            ("oic acid", "tetroic acid"),
            ("ene", "tetraene"),
            ("imine", "tetraimine"),
            ("uide", "tetrauide"),
            ("yl", "tetrayl"),
            ("carboxylic acid", "tetracarboxylic acid"),
        ] {
            assert_eq!(suffix(4, ending), expected, "{ending}");
        }
        assert_eq!(basic(4), "tetra");
    }

    #[test]
    fn suffix_multiplier_handles_single_and_larger_counts() {
        for (count, ending, expected) in [
            (1, "one", "one"),
            (2, "one", "dione"),
            (3, "one", "trione"),
            (6, "ol", "hexol"),
            (11, "one", "undecone"),
            (20, "one", "icosone"),
            (1000, "one", "kilione"),
        ] {
            assert_eq!(suffix(count, ending), expected, "{count} {ending}");
        }
    }
}
