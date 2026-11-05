//! Graph-backed ionic spelling changes from upstream `ionic_naming.py`.
use super::{
    assembly_parts::{AnionSuffix, AssemblyParts, SubstituentItem},
    assembly_utils::{is_fully_enclosed, parse_locant},
    molecule::{AtomSet, Molecule},
    nomenclature::RULES,
};
use crate::naming::{NameReason, NameStatus};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone)]
struct ChargeSite {
    atom: usize,
    locant: String,
    symbol: String,
    charge: i32,
}

/// Admit oxo protonation only where the naming grammar preserves its charge.
pub(super) fn validate_heteroatom_oxo_charge_states(mol: &Molecule) -> Result<(), String> {
    for (&id, atom) in &mol.atoms {
        // Upstream divergence: neutral imine suffixes do not encode a terminal
        // imine anion. Decline before parent selection can lose its N charge.
        if atom.symbol == "N"
            && atom.charge < 0
            && mol.degree(id) == 1
            && mol.get_neighbors(id).iter().any(|&neighbor| {
                mol.atoms[&neighbor].is_carbon()
                    && mol
                        .get_bond(id, neighbor)
                        .is_some_and(|bond| bond.order == 2)
            })
        {
            return Err("openclatura: anionic terminal imine charge state is not supported".into());
        }
        // Upstream divergence: oxido on an H-bearing chalcogen can be read as
        // neutral oxo, consuming both its anionic charge and the chalcogen H.
        if atom.symbol == "O"
            && atom.charge < 0
            && mol.degree(id) == 1
            && mol.get_neighbors(id).iter().any(|&neighbor| {
                let center = &mol.atoms[&neighbor];
                matches!(center.symbol.as_str(), "S" | "Se" | "Te")
                    && center.total_h_count > 0
                    && mol
                        .get_bond(id, neighbor)
                        .is_some_and(|bond| bond.order == 1)
            })
        {
            return Err("openclatura: oxido on an H-bearing chalcogen is not supported".into());
        }
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

/// Select the supported parent-anion operations before changing the neutral citation plan.
pub(super) fn prepare_anion_parent(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Result<(), String> {
    let sites = sites(mol, path, locants)?;
    let represented: AtomSet = parts
        .a_prefixes
        .iter()
        .flat_map(|prefix| prefix.charge_atom_ids.iter().copied())
        .collect();
    let negative: Vec<_> = sites
        .iter()
        .filter(|site| site.charge == -1 && !represented.contains(&site.atom))
        .collect();
    if negative.is_empty() {
        return Ok(());
    }
    let zwitterion = ring_nitrogen_zwitterion(mol, parts, path, &sites);
    // Leave a second site uncited for validation, as the old per-site passes did.
    let negative = if zwitterion {
        negative.as_slice()
    } else {
        &negative[..1]
    };
    let (_, _, neutral_cites_locants) =
        super::assembly_parent::format_principal_suffix(parts, "e")?;
    // Benzene is otherwise recognised only during assembly.
    super::assembly_parent::promote_benzene_retained_name(parts);
    // Retained functional parents (phenol, benzoic acid) and contracted
    // substituent stems (phenyl) have no anion spelling yet. Numbering does
    // not yet give `ide` the lowest locant (P-31.1.4.2.4), so a substituted
    // parent is cited only where its numbering fixes the anionic centre.
    let hydro = |key: &str| parts.hydro_operations.iter().any(|op| op.key == key);
    let retained_supported = parts.retained_name.as_deref().is_some_and(|name| {
        !(name == "benzene" && parts.principal_group.is_some())
            && !(parts.is_substituent
                && super::retained_specs::retained_parent_spec(Some(name))
                    .is_some_and(|spec| spec.substituent_stem.is_some()))
            && (parts.substituents.is_empty()
                || !parts.indicated_hydrogens.is_empty()
                // The retained parent can carry its indicated H in the
                // hydride citation itself, including at a substituted anion
                // site with no hydrogen left on the observed atom.
                || parts.retained_parent_metadata.as_ref().is_some_and(|metadata| {
                    negative
                        .iter()
                        .all(|site| metadata.default_indicated_h.contains(&site.locant))
                })
                || negative.iter().all(|site| {
                    site.locant == "1"
                        || site.symbol != "C"
                            && parts
                                .parent_atom_symbols_by_locant
                                .values()
                                .filter(|symbol| **symbol == site.symbol)
                                .count()
                                == 1
                }))
            // P-72.2.2.1.1 names a hydro-mancude anion with added hydrogen
            // (`pyridin-1(2H)-ide`), which is not composed yet.
            && !(parts.indicated_hydrogens.is_empty() && hydro("additive_hydrogen"))
            // A suffix with added hydrogen (`pyridin-1-id-2(1H)-one`) is cited
            // only on the bare parent, as before; substituted or hydro forms
            // need the added-hydrogen construction too.
            && !(hydro("added_hydrogen")
                && (!parts.substituents.is_empty() || hydro("additive_hydrogen")))
    });
    // A saturated substituent that cites its stem and attachment (`propan-2-yl`).
    let saturated_attachment = parts.is_substituent
        && !parts.is_acyl_substituent
        && parts.attachment_order == 1
        && parts.retained_name.is_none()
        && parts.unsaturations.is_empty()
        && parts.parent_length > 1
        && (parts.attachment_locant != "1"
            || super::assembly_parent::always_print_substituent_locant(parts));
    let group = parts.principal_group.as_ref();
    // Ketone hydrogen on a mancude ring (`pyridin-2(1H)-one`) needs added
    // hydrogen once the ketone becomes oxo (P-72.2.2.1.1); keep the suffix.
    let single_ketone = neutral_cites_locants
        && parts.hydro_operations.is_empty()
        && group.is_some_and(|group| group.key == "ketone" && group.locants.len() == 1);
    let supported = |site: &&ChargeSite| {
        zwitterion
            || retained_supported
            || parts.parent_length == 1
                && site.symbol == "C"
                && group.is_none()
                && !parts.is_substituent
            || single_ketone
            || group.is_some_and(|group| match (group.key.as_str(), group.locants.len()) {
                ("aldehyde", 1) => {
                    site.symbol == "C" && !parts.is_ring && parts.unsaturations.is_empty()
                }
                ("ring_aldehyde", 1) => site.symbol == "C" && neutral_cites_locants,
                ("nitrile", 1) => site.symbol == "C" && !parts.is_ring,
                ("nitrile", 2) => {
                    site.symbol == "C" && !parts.is_ring && parts.unsaturations.is_empty()
                }
                ("ring_nitrile", 1) => site.symbol == "C",
                ("ketone" | "amine", 2) | ("aminium" | "iminium" | "diazonio", 1) => {
                    neutral_cites_locants
                }
                _ => false,
            })
            || site.symbol == "C" && site.locant == parts.attachment_locant && saturated_attachment
    };
    if !negative.iter().all(supported) {
        return Ok(());
    }
    let mut anion_locants: Vec<_> = negative.iter().map(|site| site.locant.clone()).collect();
    anion_locants.sort_by_key(|locant| parse_locant(locant));
    parts.anion_suffix = Some(AnionSuffix {
        locants: anion_locants,
        cite_locants: !(parts.parent_length == 1
            || parts.retained_name.as_deref() == Some("benzene")
                && parts.principal_group.is_none()
                && parts.substituents.is_empty()
                && !parts.is_substituent),
    });
    if single_ketone && let Some(group) = parts.principal_group.take() {
        let mut prefix = SubstituentItem::new(
            RULES
                .functional_groups
                .get(&group.key)
                .and_then(|rule| rule.prefix.as_ref())
                .map_or("oxo", String::as_str),
            group.locants,
        );
        prefix.atom_ids = group.atom_ids;
        prefix.bond_ids = group.bond_ids;
        prefix.charge_atom_ids = group.charge_atom_ids;
        parts.substituents.push(prefix);
    }
    Ok(())
}

/// Reject parent charges the assembled name does not cite.
pub(super) fn validate_anionic_parent(
    mol: &Molecule,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
    parts: &AssemblyParts,
    represented: &AtomSet,
) -> Result<(), String> {
    let sites = sites(mol, path, locants)?;
    if parts.retained_name.is_some() {
        validate_retained_chalcogen_cation_pi_state(mol, path, &sites)?;
    }
    // A parent charge must have an exact supported suffix or replacement operation.
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
    let mut covered = represented.clone();
    if let Some(anion) = &parts.anion_suffix {
        covered.extend(
            anion
                .locants
                .iter()
                .filter_map(|locant| parts.parent_atom_ids_by_locant.get(locant).copied()),
        );
    }
    for site in sites.iter().filter(|site| site.charge < 0) {
        if !covered.contains(&site.atom) {
            return Err(format!(
                "openclatura: anionic {} parent atom {} has no supported charge citation",
                site.symbol, site.atom
            ));
        }
    }
    Ok(())
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

fn ring_nitrogen_zwitterion(
    mol: &Molecule,
    parts: &AssemblyParts,
    path: &[usize],
    sites: &[ChargeSite],
) -> bool {
    if parts.principal_group.is_some()
        || super::assembly_charge::parent_charge_suffix(parts).is_empty()
    {
        return false;
    }
    let parent: AtomSet = path.iter().copied().collect();
    let bonds: Vec<_> = mol
        .bonds
        .values()
        .filter(|bond| parent.contains(&bond.u) && parent.contains(&bond.v))
        .collect();
    if parent.len() < 3 || bonds.len() < parent.len() || !bonds.iter().any(|bond| bond.order > 1) {
        return false;
    }
    let negative: Vec<_> = sites.iter().filter(|site| site.charge < 0).collect();
    let positive: Vec<_> = sites
        .iter()
        .filter(|site| site.symbol == "N" && site.charge > 0)
        .collect();
    // The old route rewrote a single unmultiplied `-N-ium` suffix only.
    !negative.is_empty()
        && positive.len() == 1
        && negative.iter().all(|anion| {
            anion.symbol == "N"
                && positive
                    .iter()
                    .any(|cation| adjacent_or_conjugated(mol, &parent, anion.atom, cation.atom))
        })
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

pub(super) fn apply_cationic_imino_parent_prefixes(
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

pub(super) fn apply_cationic_imino_names(name: &str, mol: &Molecule) -> Result<String, String> {
    let first = mol
        .atoms
        .iter()
        .filter(|(_, a)| a.symbol == "N" && a.charge > 0)
        .find_map(|(&idx, _)| nitrogen_has_carbon_double(mol, idx).map(|carbon| (idx, carbon)));
    let Some(_) = first else {
        return Ok(name.into());
    };
    if mol.atoms.iter().any(|(&idx, a)| {
        a.symbol == "N" && a.charge == 0 && nitrogen_has_carbon_double(mol, idx).is_some()
    }) {
        return Ok(name.into());
    }
    Ok(name
        .replace("(imino)methyl", "(iminio)methyl")
        .replace("iminomethyl", "iminiomethyl"))
}

/// The accepted ammonio fallback cites its methylidene ligand on N while the
/// ligand and its attachment graph are still available to the renderer.
pub(super) fn cationic_methylidene_fallback(
    mol: &Molecule,
    nitrogen: usize,
    roots: &[usize],
    names: &[String],
) -> Option<String> {
    let ([carbon], [ligand]) = (roots, names) else {
        return None;
    };
    if mol.atoms[&nitrogen].charge != 1
        || !mol.atoms[carbon].is_carbon()
        || mol.bond_order(nitrogen, *carbon) != 2
        || mol
            .get_neighbors(*carbon)
            .iter()
            .filter(|&&n| n != nitrogen && matches!(mol.atoms[&n].symbol.as_str(), "N" | "O" | "S"))
            .count()
            < 2
    {
        return None;
    }
    mol.mark_name(NameStatus::Unverified(NameReason::CationPrefix));
    Some(format!(
        "(N-({})ammonio)",
        super::formatting::strip_outer_parentheses(ligand)
    ))
}

#[cfg(test)]
mod tests {
    use super::super::molecule::{Atom, Bond};
    use super::*;

    #[test]
    fn retained_chalcogen_cations_need_a_proved_ring_pi_state() {
        for (smiles, expected) in [
            ("[SH+]1CNCC1", "1,3-thiazolidin-1-ium"),
            ("[SeH+]1CNCC1", "1,3-selenazolidin-1-ium"),
            ("[TeH+]1CNCC1", "1,3-tellurazolidin-1-ium"),
        ] {
            assert_eq!(super::super::name_smiles(smiles).unwrap().text, expected);
        }
        for smiles in [
            "[S+]1=CN=CC1",
            "[Se+]1=CN=CC1",
            "[Te+]1=CN=CC1",
            "CC1=NC=[S+]C1",
        ] {
            assert!(
                super::super::name_smiles(smiles)
                    .unwrap_err()
                    .contains("chalcogen cation pi state"),
                "{smiles}",
            );
        }
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
    fn external_ligands_cannot_lose_charge_or_chalcogen_hydrogen() {
        use super::super::graph_io::read_smiles;
        for smiles in ["[O-][SH]1c2ccc3cc1ccc3c2", "[N-]=C1CCCCC1"] {
            let mol = read_smiles(smiles).unwrap();
            assert!(
                validate_heteroatom_oxo_charge_states(&mol).is_err(),
                "{smiles}"
            );
        }
        for smiles in [
            "C[S+](C)[O-]",
            "N=C1CCCCC1",
            "[NH2+]=C1CCCCC1",
            "[SH2]1c2ccc3cc1ccc3c2",
        ] {
            let mol = read_smiles(smiles).unwrap();
            assert!(
                validate_heteroatom_oxo_charge_states(&mol).is_ok(),
                "{smiles}"
            );
        }
    }

    #[test]
    fn cationic_imino_spelling_uses_the_ligand_graph() {
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
            cationic_methylidene_fallback(
                &mol, 0, &[1], &["((amino)(methoxy)methylidene)".into()],
            ).as_deref(),
            Some("(N-((amino)(methoxy)methylidene)ammonio)")
        );
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::CationPrefix)
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
            .map(|name| name.text)
            .as_deref(),
            Ok("1-[(1-imino-1-phenylmethyl){[(1S)-6-methyl-2-azabicyclo[4.3.1]decan-8-yl]methylidene}ammonio]ethan-1-iminium"),
        );
    }
}
