//! Graph-backed ionic spelling changes from upstream `ionic_naming.py`.
use super::{
    assembly_utils::{is_fully_enclosed, needs_hyphen},
    molecule::{AtomSet, Molecule},
    nomenclature::RULES,
};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone)]
struct ChargeSite {
    atom: usize,
    locant: String,
    symbol: String,
    charge: i32,
}

/// Admit oxo protonation only where the naming grammar preserves its charge.
pub fn validate_heteroatom_oxo_charge_states(mol: &Molecule) -> Result<(), String> {
    for (&id, atom) in &mol.atoms {
        if atom.symbol != "O" || atom.charge != 1 || atom.total_h_count == 0 {
            continue;
        }
        if mol.get_neighbors(id).iter().any(|&neighbor| {
            mol.atoms
                .get(&neighbor)
                .is_some_and(|other| !other.is_carbon())
                && mol
                    .get_bond(id, neighbor)
                    .is_some_and(|bond| bond.order == 2)
        }) {
            // Neutral oxo functional-group prefixes do not carry the O-bound
            // proton or its charge. Those groups need a typed protonation
            // operation before they can be emitted without losing chemistry.
            return Err(
                "openclatura: protonated noncarbon oxo charge state is not supported".into(),
            );
        }
    }
    Ok(())
}

fn sites(
    mol: &Molecule,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Result<Vec<ChargeSite>, String> {
    path.iter()
        .filter_map(|idx| match mol.atoms.get(idx) {
            Some(atom) if atom.charge != 0 => Some(
                locants
                    .get(idx)
                    .map(|locant| ChargeSite {
                        atom: *idx,
                        locant: locant.clone(),
                        symbol: atom.symbol.clone(),
                        charge: atom.charge,
                    })
                    .ok_or_else(|| format!("Missing charged-parent locant: {idx}")),
            ),
            Some(_) => None,
            None => Some(Err(format!("Missing charged-parent atom: {idx}"))),
        })
        .collect()
}

pub fn apply_anionic_parent_names(
    name: &str,
    mol: &Molecule,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
    retained: Option<&str>,
    represented: &AtomSet,
) -> Result<String, String> {
    let sites = sites(mol, path, locants)?;
    if retained.is_some() {
        validate_retained_chalcogen_cation_pi_state(mol, path, &sites)?;
    }
    // Upstream divergence: a parent charge must have an exact supported suffix or replacement operation.
    if let Some(site) = sites.iter().find(|site| {
        site.charge != -1
            && (site.charge != 1
                || !matches!(
                    site.symbol.as_str(),
                    "N" | "O" | "S" | "Se" | "Te" | "P" | "As" | "Sb" | "Bi"
                ))
    }) {
        return Err(format!(
            "openclatura: parent charge {} on {} atom {} is not supported",
            site.charge, site.symbol, site.atom
        ));
    }
    let mut updated = ring_nitrogen_zwitterion_stack(name, mol, path, &sites);
    let mut covered = represented.clone();
    if updated != name {
        covered.extend(
            sites
                .iter()
                .filter(|s| s.symbol == "N" && s.charge == -1)
                .map(|s| s.atom),
        );
    }
    // The wrapper selects negative sites only, retaining numbered-path order.
    for site in sites.iter().filter(|site| site.charge < 0) {
        let next = apply_parent_anion_rule(&updated, site, retained);
        // Upstream divergence: atom membership alone cannot credit an anion whose name never changes.
        if next == updated && !covered.contains(&site.atom) {
            return Err(format!(
                "openclatura: anionic {} parent atom {} has no supported charge citation",
                site.symbol, site.atom
            ));
        }
        updated = next;
    }
    Ok(updated)
}

fn validate_retained_chalcogen_cation_pi_state(
    mol: &Molecule,
    path: &[usize],
    sites: &[ChargeSite],
) -> Result<(), String> {
    let parent: AtomSet = path.iter().copied().collect();
    for site in sites {
        if site.charge != 1
            || !matches!(site.symbol.as_str(), "S" | "Se" | "Te")
            || mol
                .atoms
                .get(&site.atom)
                .is_none_or(|atom| atom.is_aromatic)
        {
            continue;
        }
        let neighbors: AtomSet = mol
            .get_neighbors(site.atom)
            .iter()
            .copied()
            .filter(|neighbor| parent.contains(neighbor))
            .collect();
        for &pi_neighbor in &neighbors {
            if mol
                .get_bond(site.atom, pi_neighbor)
                .is_none_or(|bond| bond.order < 2)
            {
                continue;
            }
            let mut queue = VecDeque::from([pi_neighbor]);
            let mut visited = AtomSet::from([site.atom, pi_neighbor]);
            while let Some(atom) = queue.pop_front() {
                for &next in mol.get_neighbors(atom) {
                    if !parent.contains(&next) || !visited.insert(next) {
                        continue;
                    }
                    if neighbors.contains(&next) {
                        // A neutral retained chalcogen parent keeps its ring
                        // bonds single. An ium suffix alone does not prove the
                        // changed pi bonds or the corresponding hydrogen sites;
                        // a parser can otherwise reconstruct another tautomer.
                        return Err(
                            "openclatura: retained cyclic chalcogen cation pi state is not supported"
                                .into(),
                        );
                    }
                    queue.push_back(next);
                }
            }
        }
    }
    Ok(())
}

fn locant_key(locant: &str) -> (usize, String) {
    if !locant.is_empty() && locant.bytes().all(|b| b.is_ascii_digit()) {
        (locant.parse().unwrap_or(999), String::new())
    } else {
        (999, locant.into())
    }
}

fn sorted_locants(sites: &[&ChargeSite]) -> String {
    let mut locants = sites.iter().map(|s| s.locant.as_str()).collect::<Vec<_>>();
    locants.sort_by_key(|locant| locant_key(locant));
    locants.join(",")
}

fn ring_nitrogen_zwitterion_stack(
    name: &str,
    mol: &Molecule,
    path: &[usize],
    sites: &[ChargeSite],
) -> String {
    if !name.contains("-ium") || name.contains("-ide") {
        return name.into();
    }
    let parent: AtomSet = path.iter().copied().collect();
    let bonds = mol
        .bonds
        .values()
        .filter(|b| parent.contains(&b.u) && parent.contains(&b.v))
        .collect::<Vec<_>>();
    if parent.len() < 3 || bonds.len() < parent.len() || !bonds.iter().any(|b| b.order > 1) {
        return name.into();
    }
    let negative = sites
        .iter()
        .filter(|s| s.symbol == "N" && s.charge < 0)
        .collect::<Vec<_>>();
    let positive = sites
        .iter()
        .filter(|s| s.symbol == "N" && s.charge > 0)
        .collect::<Vec<_>>();
    if negative.is_empty() || positive.is_empty() {
        return name.into();
    }
    let positive_text = sorted_locants(&positive);
    let suffix = format!("-{positive_text}-ium");
    // Upstream requires the first matching parent-ium marker to be terminal.
    let first_parent_ium = name
        .match_indices(&suffix)
        .find(|(start, _)| word_boundary(name, start + suffix.len()))
        .map(|(start, _)| start);
    if first_parent_ium != name.len().checked_sub(suffix.len()) {
        return name.into();
    }
    if !negative.iter().all(|a| {
        positive
            .iter()
            .any(|b| adjacent_or_conjugated(mol, &parent, a.atom, b.atom))
    }) {
        return name.into();
    }
    let negative_text = sorted_locants(&negative);
    name.replacen(
        &suffix,
        &format!("-{negative_text}-ide-{positive_text}-ium"),
        1,
    )
}

fn adjacent_or_conjugated(mol: &Molecule, parent: &AtomSet, anion: usize, cation: usize) -> bool {
    if mol.get_neighbors(anion).contains(&cation) {
        return true;
    }
    let mut queue = VecDeque::from([(anion, 0)]);
    let mut visited = AtomSet::from([anion]);
    while let Some((idx, distance)) = queue.pop_front() {
        if distance >= 3 {
            continue;
        }
        for &neighbor in mol.get_neighbors(idx) {
            if !parent.contains(&neighbor)
                || visited.contains(&neighbor)
                || !mol
                    .get_bond(idx, neighbor)
                    .is_some_and(|b| matches!(b.order, 1 | 2))
            {
                continue;
            }
            if neighbor == cation {
                return true;
            }
            visited.insert(neighbor);
            queue.push_back((neighbor, distance + 1));
        }
    }
    false
}

#[derive(Clone, Copy)]
enum StemGrammar {
    Simple,
    Rich,
    Ketone,
}

fn simple(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b",-[]^{}".contains(&b)
}
fn rich(b: u8) -> bool {
    simple(b) || matches!(b, b'.' | b'\'')
}
fn word_boundary(text: &str, end: usize) -> bool {
    text.get(end..)
        .and_then(|s| s.chars().next())
        .is_none_or(|c| !c.is_alphanumeric() && c != '_')
}
fn ascii_left_boundary(text: &str, start: usize) -> bool {
    start == 0
        || text
            .as_bytes()
            .get(start - 1)
            .is_none_or(|b| !b.is_ascii_alphanumeric())
}
fn digits_end(text: &str, start: usize) -> Option<usize> {
    let count = text
        .get(start..)?
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    (count > 0).then_some(start + count)
}
fn indicated_end(text: &str, start: usize) -> Option<usize> {
    let end = digits_end(text, start)?;
    text.get(end..)?.starts_with("H-").then_some(end + 2)
}

/// Walk the same ASCII parent-token grammar used by upstream's non-greedy
/// regular expressions, including parenthesized numeric fused-ring locants.
fn stem_ends(text: &str, start: usize, grammar: StemGrammar) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut cursor = start;
    while let Some(&b) = text.as_bytes().get(cursor) {
        let allowed = match grammar {
            StemGrammar::Simple => simple(b),
            StemGrammar::Rich => rich(b),
            StemGrammar::Ketone if cursor == start => {
                b.is_ascii_alphanumeric() || b",[]^{}".contains(&b)
            }
            StemGrammar::Ketone => rich(b),
        };
        if allowed {
            cursor += 1;
        } else if cursor > start && !matches!(grammar, StemGrammar::Simple) && b == b'(' {
            let Some(end) = digits_end(text, cursor + 1) else {
                break;
            };
            if text.as_bytes().get(end) != Some(&b')') {
                break;
            }
            cursor = end + 1;
        } else {
            break;
        }
        ends.push(cursor);
    }
    ends
}

// Match callbacks return the consumed suffix length and the replacement. The
// scanner preserves Python re.sub's leftmost/non-greedy match and count order.
fn rewrite_stem_suffix(
    text: &str,
    grammar: StemGrammar,
    all: bool,
    mut apply: impl FnMut(usize, &str, &str, &str) -> Option<(usize, String)>,
) -> String {
    let mut output = String::new();
    let mut consumed = 0;
    for (start, _) in text.char_indices() {
        if start < consumed
            || (matches!(grammar, StemGrammar::Ketone) && !ascii_left_boundary(text, start))
        {
            continue;
        }
        let indicated = if matches!(grammar, StemGrammar::Ketone) {
            indicated_end(text, start)
        } else {
            None
        };
        let alternatives = indicated.into_iter().chain(std::iter::once(start));
        let mut matched = None;
        for stem_start in alternatives {
            for end in stem_ends(text, stem_start, grammar) {
                let (Some(stem), Some(tail), Some(prefix)) = (
                    text.get(stem_start..end),
                    text.get(end..),
                    text.get(start..stem_start),
                ) else {
                    continue;
                };
                if let Some((length, replacement)) = apply(start, prefix, stem, tail) {
                    matched = Some((end + length, replacement));
                    break;
                }
            }
            if matched.is_some() {
                break;
            }
        }
        if let Some((end, replacement)) = matched {
            output.push_str(text.get(consumed..start).unwrap_or_default());
            output.push_str(&replacement);
            consumed = end;
            if !all {
                break;
            }
        }
    }
    output.push_str(text.get(consumed..).unwrap_or_default());
    output
}

fn suffix_match(tail: &str, suffix: &str) -> bool {
    tail.starts_with(suffix) && word_boundary(tail, suffix.len())
}
fn locanted_suffix<'a>(tail: &'a str, suffix: &str) -> Option<(&'a str, usize)> {
    if !tail.starts_with('-') {
        return None;
    }
    let end = digits_end(tail, 1)?;
    let terminal = format!("-{suffix}");
    if !tail.get(end..)?.starts_with(&terminal) || !word_boundary(tail, end + terminal.len()) {
        return None;
    }
    Some((tail.get(1..end)?, end + terminal.len()))
}

fn explicit_implicit_cation_locant(stem: &str) -> String {
    for (neutral, ionic) in &RULES.charges.retained_ionic_n_parents {
        if let Some(prefix) = stem.strip_suffix(ionic) {
            let neutral = neutral.strip_suffix('e').unwrap_or(neutral);
            return format!("{prefix}{neutral}-1-ium");
        }
    }
    stem.into()
}

fn accepts(key: &str, symbol: &str) -> bool {
    RULES
        .charges
        .anion_suffix_placements
        .iter()
        .find(|rule| rule.get("key").and_then(serde_json::Value::as_str) == Some(key))
        .and_then(|rule| rule.get("atom_symbols"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|symbols| {
            symbols
                .iter()
                .any(|s| matches!(s.as_str(), Some("*")) || s.as_str() == Some(symbol))
        })
}

fn apply_suffix_rule(name: &str, site: &ChargeSite, key: &str) -> String {
    let operation = format!("-{}-ide", site.locant);
    if name.contains(&operation) || !accepts(key, &site.symbol) {
        return name.into();
    }
    let locant = site.locant.as_str();
    match key {
        "ketone_to_oxo_ide" => rewrite_stem_suffix(
            name,
            StemGrammar::Ketone,
            true,
            |start, indicated, stem, tail| {
                if !tail.starts_with('-') {
                    return None;
                }
                let oxo_end = digits_end(tail, 1)?;
                if !tail.get(oxo_end..)?.starts_with("-one") {
                    return None;
                }
                let length = oxo_end + 4;
                let oxo = tail.get(1..oxo_end)?;
                // This pattern rejects letters after 'one', but permits digits.
                if tail
                    .get(length..)
                    .and_then(|s| s.chars().next())
                    .is_some_and(|c| c.is_ascii_alphabetic())
                {
                    return None;
                }
                let parent = explicit_implicit_cation_locant(stem);
                let tail = format!("{indicated}{parent}-{locant}-ide");
                let joint = if needs_hyphen(&format!("{oxo}-oxo"), &tail) {
                    "-"
                } else {
                    ""
                };
                let separator = if start > 0
                    && name
                        .as_bytes()
                        .get(start - 1)
                        .is_some_and(|b| !b"- ".contains(b))
                {
                    "-"
                } else {
                    ""
                };
                Some((length, format!("{separator}{oxo}-oxo{joint}{tail}")))
            },
        ),
        "aldehyde_to_ide_al" => {
            rewrite_stem_suffix(name, StemGrammar::Simple, false, |_, _, stem, tail| {
                (stem.len() >= 3 && stem.ends_with("an") && suffix_match(tail, "al"))
                    .then(|| (2, format!("{stem}-{locant}-ide-1-al")))
            })
        }
        "carbaldehyde_suffix_ide" => {
            rewrite_stem_suffix(name, StemGrammar::Rich, false, |_, _, stem, tail| {
                let (suffix_locant, length) = locanted_suffix(tail, "carbaldehyde")?;
                Some((
                    length,
                    format!(
                        "{}-{locant}-ide-{suffix_locant}-carbaldehyde",
                        explicit_implicit_cation_locant(stem)
                    ),
                ))
            })
        }
        "dinitrile_suffix_ide" => {
            rewrite_stem_suffix(name, StemGrammar::Simple, false, |_, _, stem, tail| {
                (stem.len() >= 4 && stem.ends_with("ane") && suffix_match(tail, "dinitrile"))
                    .then(|| (9, format!("{stem}-{locant}-ide-1,3-dinitrile")))
            })
        }
        "mononitrile_suffix_ide" => {
            rewrite_stem_suffix(name, StemGrammar::Simple, false, |_, _, stem, tail| {
                (stem.len() >= 4
                    && ["ane", "ene", "yne"]
                        .iter()
                        .any(|ending| stem.ends_with(ending))
                    && suffix_match(tail, "nitrile"))
                .then(|| (7, format!("{stem}-{locant}-ide-1-nitrile")))
            })
        }
        "locanted_carbonitrile_suffix_ide" => {
            rewrite_stem_suffix(name, StemGrammar::Simple, false, |_, _, stem, tail| {
                let (suffix_locant, length) = locanted_suffix(tail, "carbonitrile")?;
                Some((
                    length,
                    format!("{stem}-{locant}-ide-{suffix_locant}-carbonitrile"),
                ))
            })
        }
        "terminal_carbonitrile_suffix_ide" => {
            rewrite_stem_suffix(name, StemGrammar::Simple, false, |_, _, stem, tail| {
                suffix_match(tail, "carbonitrile")
                    .then(|| (12, format!("{stem}-{locant}-ide-carbonitrile")))
            })
        }
        "terminal_characteristic_suffix_ide" => {
            let mut result = name.to_owned();
            for suffix in ["dione", "diamine", "aminium", "iminium", "diazonium"] {
                let Some(stem) = name.strip_suffix(&format!("-{suffix}")) else {
                    continue;
                };
                let Some((parent, suffix_locants)) = stem.rsplit_once('-') else {
                    continue;
                };
                if !parent.is_empty()
                    && !suffix_locants.is_empty()
                    && suffix_locants
                        .split(',')
                        .all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit()))
                {
                    result = format!("{parent}{operation}-{suffix_locants}-{suffix}");
                    break;
                }
            }
            result
        }
        _ => name.into(),
    }
}

fn apply_parent_anion_rule(name: &str, site: &ChargeSite, retained: Option<&str>) -> String {
    let mut updated = name.to_owned();
    // Keep the precise order of upstream's separate suffix-stack calls.
    for key in [
        "ketone_to_oxo_ide",
        "aldehyde_to_ide_al",
        "carbaldehyde_suffix_ide",
        "dinitrile_suffix_ide",
        "mononitrile_suffix_ide",
        "locanted_carbonitrile_suffix_ide",
        "terminal_carbonitrile_suffix_ide",
        "terminal_characteristic_suffix_ide",
    ] {
        updated = apply_suffix_rule(&updated, site, key);
    }
    if site.symbol == "C" {
        let target = format!("-{}-yl", site.locant);
        updated = rewrite_stem_suffix(&updated, StemGrammar::Simple, false, |_, _, stem, tail| {
            (stem.len() >= 3 && stem.ends_with("an") && suffix_match(tail, &target)).then(|| {
                (
                    target.len(),
                    format!("{stem}-{}-ide{}", site.locant, target),
                )
            })
        });
    }
    if updated != name {
        return updated;
    }
    let Some(stem) = retained
        .map(|s| s.strip_suffix('e').unwrap_or(s))
        .filter(|s| !s.is_empty())
    else {
        return name.into();
    };
    replace_retained_stem_with_ide(name, stem, &site.locant)
    // Unclassified terminal locant-ide suffixes are intentionally never added.
}

fn indicated_h_prefix_end(text: &str, start: usize) -> Option<usize> {
    let mut cursor = start;
    loop {
        let end = digits_end(text, cursor)?;
        if text.as_bytes().get(end) != Some(&b'H') {
            return None;
        }
        match text.as_bytes().get(end + 1) {
            Some(b',') => cursor = end + 2,
            Some(b'-') => return Some(end + 2),
            _ => return None,
        }
    }
}

fn replace_retained_stem_with_ide(name: &str, stem: &str, locant: &str) -> String {
    let mut output = String::new();
    let mut consumed = 0;
    for (start, _) in name.char_indices() {
        if start < consumed || !ascii_left_boundary(name, start) {
            continue;
        }
        let indicated = indicated_h_prefix_end(name, start);
        let mut matched = None;
        for stem_start in indicated.into_iter().chain(std::iter::once(start)) {
            if !name
                .get(stem_start..)
                .is_some_and(|tail| tail.starts_with(stem))
            {
                continue;
            }
            let stem_end = stem_start + stem.len();
            let with_e = name.as_bytes().get(stem_end) == Some(&b'e');
            for end in [stem_end + usize::from(with_e), stem_end] {
                let tail = name.get(end..).unwrap_or_default();
                let already_ide = if tail.starts_with('-') {
                    digits_end(tail, 1)
                        .and_then(|end| tail.get(end..))
                        .is_some_and(|tail| tail.starts_with("-ide"))
                } else {
                    false
                };
                if already_ide {
                    continue;
                }
                let prefix = name
                    .get(start..stem_start)
                    .unwrap_or_default()
                    .strip_suffix('-')
                    .unwrap_or_default();
                let kept = prefix
                    .split(',')
                    .filter(|token| !token.is_empty() && token.strip_suffix('H') != Some(locant))
                    .collect::<Vec<_>>();
                let prefix = if kept.is_empty() {
                    String::new()
                } else {
                    format!("{}-", kept.join(","))
                };
                matched = Some((end, format!("{prefix}{stem}-{locant}-ide")));
                break;
            }
            if matched.is_some() {
                break;
            }
        }
        if let Some((end, replacement)) = matched {
            output.push_str(name.get(consumed..start).unwrap_or_default());
            output.push_str(&replacement);
            consumed = end;
        }
    }
    output.push_str(name.get(consumed..).unwrap_or_default());
    output
}

fn nitrogen_has_carbon_double(mol: &Molecule, idx: usize) -> Option<usize> {
    mol.get_neighbors(idx).iter().copied().find(|&n| {
        mol.atoms.get(&n).is_some_and(|a| a.is_carbon())
            && mol.get_bond(idx, n).is_some_and(|b| b.order == 2)
    })
}

fn belongs_to_parent_scope(name: &str, position: usize) -> bool {
    let mut scope = name;
    let mut scope_start = 0;
    while is_fully_enclosed(scope) {
        scope = &scope[1..scope.len() - 1];
        scope_start += 1;
    }
    let Some(prefix) = position
        .checked_sub(scope_start)
        .and_then(|relative| scope.get(..relative))
    else {
        return false;
    };
    let depth = prefix
        .chars()
        .fold(0_i32, |depth, character| match character {
            '(' => depth + 1,
            ')' => depth - 1,
            _ => depth,
        });
    depth == 0
}

pub fn apply_cationic_imino_parent_prefixes(
    name: &str,
    mol: &Molecule,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Result<String, String> {
    let parent: AtomSet = path.iter().copied().collect();
    let mut updated = name.to_owned();
    for (&idx, atom) in &mol.atoms {
        if atom.symbol != "N" || atom.charge <= 0 {
            continue;
        }
        for &neighbor in mol.get_neighbors(idx) {
            if !parent.contains(&neighbor)
                || !mol.atoms.get(&neighbor).is_some_and(|a| a.is_carbon())
                || !mol.get_bond(idx, neighbor).is_some_and(|b| b.order == 2)
            {
                continue;
            }
            let locant = locants
                .get(&neighbor)
                .ok_or_else(|| format!("Missing cationic-imino parent locant: {neighbor}"))?;
            let pattern = format!("{locant}-imino");
            let replacement = format!("{locant}-iminio");
            let mut output = String::new();
            let mut consumed = 0;
            for (start, _) in updated.match_indices(&pattern) {
                let end = start + pattern.len();
                // Upstream divergence: equal numeric locants in nested branches
                // belong to different atoms. Each branch handles its own imino
                // charge before it is inserted into the enclosing parent name.
                if ascii_left_boundary(&updated, start)
                    && word_boundary(&updated, end)
                    && belongs_to_parent_scope(&updated, start)
                {
                    output.push_str(updated.get(consumed..start).unwrap_or_default());
                    output.push_str(&replacement);
                    consumed = end;
                }
            }
            output.push_str(updated.get(consumed..).unwrap_or_default());
            updated = output;
        }
    }
    Ok(updated)
}

pub fn apply_cationic_imino_names(name: &str, mol: &Molecule) -> Result<String, String> {
    let first = mol
        .atoms
        .iter()
        .filter(|(_, a)| a.symbol == "N" && a.charge > 0)
        .find_map(|(&idx, _)| nitrogen_has_carbon_double(mol, idx).map(|carbon| (idx, carbon)));
    let Some((nitrogen, carbon)) = first else {
        return Ok(name.into());
    };
    let has_two_hetero = mol
        .get_neighbors(carbon)
        .iter()
        .filter(|&&n| {
            n != nitrogen
                && mol
                    .atoms
                    .get(&n)
                    .is_some_and(|a| matches!(a.symbol.as_str(), "N" | "O" | "S"))
        })
        .count()
        >= 2;
    let updated = if has_two_hetero {
        normalize_methylideneammonio(name)
    } else {
        name.into()
    };
    if mol.atoms.iter().any(|(&idx, a)| {
        a.symbol == "N" && a.charge == 0 && nitrogen_has_carbon_double(mol, idx).is_some()
    }) {
        return Ok(updated);
    }
    Ok(updated
        .replace("(imino)methyl", "(iminio)methyl")
        .replace("iminomethyl", "iminiomethyl"))
}

fn normalize_methylideneammonio(name: &str) -> String {
    let mut output = String::new();
    let mut consumed = 0;
    for (start, _) in name.match_indices("(((") {
        if start < consumed {
            continue;
        }
        let left_start = start + 3;
        let Some(tail) = name.get(left_start..) else {
            continue;
        };
        let left_len = tail
            .bytes()
            .take_while(|&b| simple(b) || b == b'\'')
            .count();
        if left_len == 0 {
            continue;
        }
        let left_end = left_start + left_len;
        if !name.get(left_end..).is_some_and(|t| t.starts_with(")(")) {
            continue;
        }
        let right_start = left_end + 2;
        let Some(tail) = name.get(right_start..) else {
            continue;
        };
        let right_len = tail
            .bytes()
            .take_while(|&b| simple(b) || b == b'\'')
            .count();
        if right_len == 0 {
            continue;
        }
        let right_end = right_start + right_len;
        let suffix = ")methylidene)ammonio)";
        if !name.get(right_end..).is_some_and(|t| t.starts_with(suffix)) {
            continue;
        }
        let left = name.get(left_start..left_end).unwrap_or_default();
        let right = name.get(right_start..right_end).unwrap_or_default();
        output.push_str(name.get(consumed..start).unwrap_or_default());
        output.push_str(&format!("(N-(({left})({right})methylidene)ammonio)"));
        consumed = right_end + suffix.len();
    }
    output.push_str(name.get(consumed..).unwrap_or_default());
    output
}

#[cfg(test)]
mod tests {
    use super::super::molecule::{Atom, Bond};
    use super::*;

    fn one_anion(symbol: &str) -> Molecule {
        let mut mol = Molecule::new();
        let mut atom = Atom::new(0, symbol);
        atom.charge = -1;
        mol.add_atom(atom).unwrap();
        mol
    }

    #[test]
    fn retained_chalcogen_cations_need_a_proved_ring_pi_state() {
        for symbol in ["S", "Se", "Te"] {
            let mut mol = Molecule::new();
            for (id, symbol) in [(0, symbol), (1, "C"), (2, "N"), (3, "C"), (4, "C")] {
                let mut atom = Atom::new(id, symbol);
                atom.charge = i32::from(id == 0);
                mol.add_atom(atom).unwrap();
            }
            for (id, (u, v, order)) in [(0, 1, 2), (1, 2, 1), (2, 3, 2), (3, 4, 1), (4, 0, 1)]
                .into_iter()
                .enumerate()
            {
                mol.add_bond(Bond::new(id, u, v, order)).unwrap();
            }
            let path = [0, 1, 2, 3, 4];
            let locants = path.iter().map(|&id| (id, (id + 1).to_string())).collect();
            assert!(
                apply_anionic_parent_names(
                    "1,3-thiazol-1-ium",
                    &mol,
                    &path,
                    &locants,
                    Some("1,3-thiazole"),
                    &AtomSet::new(),
                )
                .unwrap_err()
                .contains("chalcogen cation pi state")
            );
            mol.atoms.get_mut(&0).unwrap().is_aromatic = true;
            assert!(
                apply_anionic_parent_names(
                    "1,3-thiazol-1-ium",
                    &mol,
                    &path,
                    &locants,
                    Some("1,3-thiazole"),
                    &AtomSet::new(),
                )
                .is_ok()
            );
            mol.atoms.get_mut(&0).unwrap().is_aromatic = false;
            mol.set_bond_order(0, 1).unwrap();
            assert!(
                apply_anionic_parent_names(
                    "1,3-thiazolidin-1-ium",
                    &mol,
                    &path,
                    &locants,
                    Some("1,3-thiazolidine"),
                    &AtomSet::new(),
                )
                .is_ok()
            );
        }
        assert!(
            super::super::name_smiles("CC1=NC=[S+]C1")
                .unwrap_err()
                .contains("chalcogen cation pi state")
        );
        assert_eq!(
            super::super::name_smiles("C[S+](C)[CH-]C").as_deref(),
            Ok("1-(dimethylsulfaniumyl)ethan-1-ide")
        );
    }

    #[test]
    fn neutral_oxo_prefixes_cannot_consume_protonated_noncarbon_oxo() {
        use super::super::graph_io::read_smiles;
        for smiles in ["CS(=[OH+])(=O)C", "OP(O)(O)=[OH+]"] {
            let mol = read_smiles(smiles).unwrap();
            assert!(
                validate_heteroatom_oxo_charge_states(&mol).is_err(),
                "{smiles}"
            );
        }
        for smiles in ["CS(=O)(=O)C", "C=[OH+]", "C[OH2+]"] {
            let mol = read_smiles(smiles).unwrap();
            assert!(
                validate_heteroatom_oxo_charge_states(&mol).is_ok(),
                "{smiles}"
            );
        }
    }

    #[test]
    fn upstream_anion_suffix_order_and_retained_spelling() {
        let mol = one_anion("C");
        for (name, retained, locant, expected) in [
            ("cyclohexan-2-one", None, "4", "2-oxocyclohexan-4-ide"),
            ("butanal", None, "2", "butan-2-ide-1-al"),
            ("propanedinitrile", None, "2", "propane-2-ide-1,3-dinitrile"),
            ("but-3-ynenitrile", None, "2", "but-3-yne-2-ide-1-nitrile"),
            (
                "cyclopentane-1-carbonitrile",
                None,
                "2",
                "cyclopentane-2-ide-1-carbonitrile",
            ),
            (
                "cyclopentanecarbonitrile",
                None,
                "2",
                "cyclopentane-2-ide-carbonitrile",
            ),
            (
                "cyclopentane-1-carbaldehyde",
                None,
                "2",
                "cyclopentane-2-ide-1-carbaldehyde",
            ),
            (
                "2-cyanopyrrolidin-3-aminium",
                None,
                "1",
                "2-cyanopyrrolidin-1-ide-3-aminium",
            ),
            (
                "1,3-dinitrilopropan-2-yl",
                None,
                "2",
                "1,3-dinitrilopropan-2-ide-2-yl",
            ),
            (
                "5-(ammoniomethyl)imidazolidine-2,4-dione",
                Some("imidazolidine"),
                "1",
                "5-(ammoniomethyl)imidazolidine-1-ide-2,4-dione",
            ),
            ("1H-pyrrole", Some("pyrrole"), "1", "pyrrol-1-ide"),
        ] {
            let actual = apply_anionic_parent_names(
                name,
                &mol,
                &[0],
                &BTreeMap::from([(0, locant.into())]),
                retained,
                &AtomSet::new(),
            )
            .unwrap();
            assert_eq!(actual, expected, "{name}");
        }
        assert!(
            apply_anionic_parent_names(
                "butane",
                &mol,
                &[0],
                &BTreeMap::from([(0, "2".into())]),
                None,
                &AtomSet::new()
            )
            .is_err()
        );
    }

    #[test]
    fn cationic_imino_rewrites_are_graph_gated() {
        let mut mol = Molecule::new();
        for (idx, symbol, charge) in [(0, "N", 1), (1, "C", 0), (2, "N", 0), (3, "O", 0)] {
            let mut atom = Atom::new(idx, symbol);
            atom.charge = charge;
            mol.add_atom(atom).unwrap();
        }
        for (idx, u, v, order) in [(1, 0, 1, 2), (2, 1, 2, 1), (3, 1, 3, 1)] {
            mol.add_bond(Bond::new(idx, u, v, order)).unwrap();
        }
        assert_eq!(
            apply_cationic_imino_names("2-(((amino)(methoxy)methylidene)ammonio)acetate", &mol)
                .unwrap(),
            "2-(N-((amino)(methoxy)methylidene)ammonio)acetate"
        );
        assert_eq!(
            apply_cationic_imino_names("3-(iminomethylamino)butanoate", &mol).unwrap(),
            "3-(iminiomethylamino)butanoate"
        );
        assert_eq!(
            apply_cationic_imino_parent_prefixes(
                "2-iminoacetate",
                &mol,
                &[1],
                &BTreeMap::from([(1, "2".into())])
            )
            .unwrap(),
            "2-iminoacetate"
        );
        // The source word boundary deliberately leaves 'iminoacetate' intact.
        assert_eq!(
            apply_cationic_imino_parent_prefixes(
                "2-imino-3-hydroxybutanoate",
                &mol,
                &[1],
                &BTreeMap::from([(1, "2".into())])
            )
            .unwrap(),
            "2-iminio-3-hydroxybutanoate"
        );
        for (name, expected) in [
            (
                "2-imino-3-(2-iminoethyl)butanoate",
                "2-iminio-3-(2-iminoethyl)butanoate",
            ),
            (
                "(2-imino-3-(2-iminoethyl)butan-1-yl)",
                "(2-iminio-3-(2-iminoethyl)butan-1-yl)",
            ),
        ] {
            assert_eq!(
                apply_cationic_imino_parent_prefixes(
                    name,
                    &mol,
                    &[1],
                    &BTreeMap::from([(1, "2".into())]),
                )
                .unwrap(),
                expected,
            );
        }
        mol.set_bond_order(2, 2).unwrap();
        assert_eq!(
            apply_cationic_imino_names("(imino)methyl", &mol).unwrap(),
            "(imino)methyl"
        );
    }

    #[test]
    fn parent_imino_charge_preserves_the_nested_neutral_imine() {
        assert_eq!(
            super::super::name_smiles(
                "CC(=[NH2+])[N+](=CC1C[C@H]2CC(C1)(CCCN2)C)C(=N)C3=CC=CC=C3",
            )
            .as_deref(),
            Ok("1-((1-imino-1-phenylmethyl)(((1S)-6-methyl-2-azabicyclo[4.3.1]decan-8-yl)methylidene)ammonio)ethan-1-iminium"),
        );
    }

    #[test]
    fn nitrogen_zwitterion_stack_uses_the_ring_graph() {
        let mut mol = Molecule::new();
        for (idx, charge) in [-1, 1, 0, -1, 1, 0].into_iter().enumerate() {
            let mut atom = Atom::new(
                idx,
                if matches!(idx, 0 | 1 | 3 | 4) {
                    "N"
                } else {
                    "C"
                },
            );
            atom.charge = charge;
            mol.add_atom(atom).unwrap();
        }
        for (idx, (u, v, order)) in [
            (0, 1, 2),
            (1, 2, 1),
            (2, 3, 2),
            (3, 4, 1),
            (4, 5, 2),
            (5, 0, 1),
        ]
        .into_iter()
        .enumerate()
        {
            mol.add_bond(Bond::new(idx + 1, u, v, order)).unwrap();
        }
        let path = [0, 1, 2, 3, 4, 5];
        let locants = path
            .iter()
            .map(|&idx| (idx, (idx + 1).to_string()))
            .collect();
        assert_eq!(
            apply_anionic_parent_names(
                "1,2,4,5-tetrazacyclohexa-1,3,5-trien-2,5-ium",
                &mol,
                &path,
                &locants,
                None,
                &AtomSet::new(),
            )
            .unwrap(),
            "1,2,4,5-tetrazacyclohexa-1,3,5-trien-1,4-ide-2,5-ium"
        );
        assert!(
            apply_anionic_parent_names(
                "1,2,4,5-tetrazacyclohexa-1,3,5-trien-2,5-ium-3-ol",
                &mol,
                &path,
                &locants,
                None,
                &AtomSet::new(),
            )
            .is_err()
        );
    }
}
