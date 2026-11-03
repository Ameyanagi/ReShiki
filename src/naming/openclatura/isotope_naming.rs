//! Isotopically substituted structures (P-82), never sample-label notation.
//!
//! The component engine supplies the parent locants. Only those positions,
//! unique heteroatoms and completely substituted elements are supported. A
//! bounded automorphism proof chooses the lowest equivalent isotope locants;
//! the finished name must independently reconstruct the original graph.
use super::{
    Name, component_namer, graph_io, locants::retained_locant_sort_key, molecule::Molecule,
};
use crate::naming::{IsotopeStyle, NameReason, NameStatus};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct Site {
    anchor: usize,
    symbol: String,
    mass: u16,
}

/// Keep CIP labels calculated with the isotopes present. Recalculating them
/// after stripping can invert an existing centre or erase an isotope-created
/// centre. The final strict parser check refuses any unsupported stereo path.
fn stripped_parent(source: &Molecule) -> Result<(Molecule, Vec<Site>), String> {
    let mut parent = Molecule::new();
    let mut sites = Vec::new();
    for (&idx, atom) in &source.atoms {
        if atom.symbol == "H" {
            let neighbors = source.get_neighbors(idx);
            if atom.charge != 0
                || neighbors.len() != 1
                || source.atoms[&neighbors[0]].symbol == "H"
                || source.bond_order(idx, neighbors[0]) != 1
            {
                return Err(
                    "openclatura: this isotopic hydrogen attachment is not supported".into(),
                );
            }
            if let Some(mass) = atom.isotope {
                sites.push(Site {
                    anchor: neighbors[0],
                    symbol: "H".into(),
                    mass,
                });
            }
            continue;
        }
        let mut stripped = atom.clone();
        if let Some(mass) = stripped.isotope.take() {
            sites.push(Site {
                anchor: idx,
                symbol: atom.symbol.clone(),
                mass,
            });
        }
        let attached_h = source
            .get_neighbors(idx)
            .iter()
            .filter(|n| source.atoms[n].symbol == "H")
            .count();
        stripped.explicit_h_count += attached_h;
        stripped.total_h_count += attached_h;
        parent.add_atom(stripped)?;
    }
    for bond in source.bonds.values() {
        if parent.atoms.contains_key(&bond.u) && parent.atoms.contains_key(&bond.v) {
            parent.add_bond(bond.clone())?;
        }
    }
    parent.accurate_cip = source.accurate_cip.clone();
    parent.legacy_cip = source.legacy_cip.clone();
    // Like the CIP labels, the possible stereo units keep isotope-created centres.
    parent.stereogenic_atoms = source
        .stereogenic_atoms
        .iter()
        .copied()
        .filter(|idx| parent.atoms.contains_key(idx))
        .collect();
    parent.stereogenic_bonds = source
        .stereogenic_bonds
        .iter()
        .copied()
        .filter(|idx| parent.bonds.contains_key(idx))
        .collect();
    if parent.atoms.is_empty() {
        return Err("openclatura: isolated isotope atoms are not supported".into());
    }
    Ok((parent, sites))
}

fn same_atom(mol: &Molecule, a: usize, b: usize) -> bool {
    let (a, b) = (&mol.atoms[&a], &mol.atoms[&b]);
    a.symbol == b.symbol
        && a.charge == b.charge
        && a.total_h_count == b.total_h_count
        && a.is_aromatic == b.is_aromatic
        && a.stereo == b.stereo
        && a.raw_stereo == b.raw_stereo
        && mol.degree(a.idx) == mol.degree(b.idx)
}

fn same_bond(mol: &Molecule, a: usize, b: usize, x: usize, y: usize) -> bool {
    match (mol.get_bond(a, b), mol.get_bond(x, y)) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.is_aromatic == b.is_aromatic
                && (a.is_aromatic || a.order == b.order)
                && a.stereo == b.stereo
        }
        _ => false,
    }
}

/// Complete labeled-graph automorphisms, rather than relying on source atom
/// order or guessing a chain direction from a SMILES string. Refuse if proving
/// completeness would exceed the request budget.
fn lowest_equivalent_map(
    mol: &Molecule,
    sites: &[Site],
    locants: &BTreeMap<usize, String>,
) -> Result<BTreeMap<usize, usize>, String> {
    mol.spend_naming_work(mol.atoms.len().saturating_mul(mol.atoms.len()))?;
    let candidates: BTreeMap<_, Vec<_>> = mol
        .atoms
        .keys()
        .map(|&a| {
            (
                a,
                mol.atoms
                    .keys()
                    .copied()
                    .filter(|&b| same_atom(mol, a, b))
                    .collect(),
            )
        })
        .collect();
    let mut order: Vec<_> = mol.atoms.keys().copied().collect();
    order.sort_by_key(|a| (candidates[a].len(), std::cmp::Reverse(mol.degree(*a))));
    struct Search<'a> {
        mol: &'a Molecule,
        candidates: &'a BTreeMap<usize, Vec<usize>>,
        order: &'a [usize],
        sites: &'a [Site],
        locants: &'a BTreeMap<usize, String>,
        best: Option<(IsotopeKey, BTreeMap<usize, usize>)>,
    }
    impl Search<'_> {
        fn visit(
            &mut self,
            mapping: &mut BTreeMap<usize, usize>,
            used: &mut BTreeSet<usize>,
        ) -> Result<(), String> {
            self.mol.spend_naming_work(1)?;
            if mapping.len() == self.order.len() {
                // A mapping that moves a label onto an atom without a locant
                // is not a usable numbering; keep searching the others.
                let Ok(key) = numbering_key(self.mol, self.sites, self.locants, mapping) else {
                    return Ok(());
                };
                if self
                    .best
                    .as_ref()
                    .is_none_or(|(previous, _)| &key < previous)
                {
                    self.best = Some((key, mapping.clone()));
                }
                return Ok(());
            }
            let atom = self.order[mapping.len()];
            for &target in &self.candidates[&atom] {
                if used.contains(&target)
                    || !mapping
                        .iter()
                        .all(|(&a, &b)| same_bond(self.mol, atom, a, target, b))
                {
                    continue;
                }
                mapping.insert(atom, target);
                used.insert(target);
                self.visit(mapping, used)?;
                used.remove(&target);
                mapping.remove(&atom);
            }
            Ok(())
        }
    }
    let mut search = Search {
        mol,
        candidates: &candidates,
        order: &order,
        sites,
        locants,
        best: None,
    };
    search.visit(&mut BTreeMap::new(), &mut BTreeSet::new())?;
    search
        .best
        .map(|(_, map)| map)
        .ok_or_else(|| "openclatura: isotope equivalent-numbering proof failed".into())
}

fn site_locant(
    mol: &Molecule,
    map: &BTreeMap<usize, String>,
    anchor: usize,
) -> Result<String, String> {
    if mol.atoms.len() == 1 {
        return Ok("1".into());
    }
    if let Some(locant) = map.get(&anchor) {
        return Ok(locant.clone());
    }
    let symbol = &mol.atoms[&anchor].symbol;
    // Letter locants can identify a unique characteristic heteroatom, but
    // neither branch atom numbers nor repeated suffix atoms can be inferred.
    if ["N", "O", "S", "Se", "Te", "P", "As"].contains(&symbol.as_str())
        && mol.atoms.values().filter(|a| &a.symbol == symbol).count() == 1
    {
        return Ok(symbol.clone());
    }
    Err("openclatura: isotope position has no proven parent or unique heteroatom locant".into())
}

type IsotopeKey = Vec<Vec<super::locants::LocantKey>>;
fn numbering_key(
    mol: &Molecule,
    sites: &[Site],
    locants: &BTreeMap<usize, String>,
    symmetry: &BTreeMap<usize, usize>,
) -> Result<IsotopeKey, String> {
    let mut all = Vec::new();
    let mut priority = BTreeMap::<(usize, u16), Vec<_>>::new();
    for site in sites {
        let locant = site_locant(mol, locants, symmetry[&site.anchor])?;
        let key = retained_locant_sort_key(&locant);
        all.push(key.clone());
        let atomic_number = super::rules::elements::get(&site.symbol)
            .ok_or("openclatura: unknown isotope element")?
            .atomic_number;
        priority
            .entry((atomic_number, site.mass))
            .or_default()
            .push(key);
    }
    all.sort();
    let mut key = vec![all];
    // P-82.5.2: first all modified positions together, then the higher atomic
    // number and, for one element, the higher mass number.
    for (_, mut locants) in priority.into_iter().rev() {
        locants.sort();
        key.push(locants);
    }
    Ok(key)
}

fn isotope_descriptors(
    mol: &Molecule,
    sites: &[Site],
    locants: &BTreeMap<usize, String>,
    symmetry: &BTreeMap<usize, usize>,
    style: IsotopeStyle,
) -> Result<String, String> {
    let mut groups = BTreeMap::<(&str, u16), Vec<usize>>::new();
    for site in sites {
        groups
            .entry((&site.symbol, site.mass))
            .or_default()
            .push(symmetry[&site.anchor]);
    }
    if style == IsotopeStyle::Boughton && groups.len() != 1 {
        return Err("openclatura: mixed-nuclide Boughton notation is not supported by the local parser; use IUPAC isotope style".into());
    }
    let mut descriptors = Vec::new();
    for ((symbol, mass), anchors) in groups {
        let available = if symbol == "H" {
            mol.atoms.values().map(|a| a.total_h_count).sum()
        } else {
            mol.atoms.values().filter(|a| a.symbol == symbol).count()
        };
        let only_carbon_parent = symbol == "H"
            && locants.len() == 1
            && anchors
                .iter()
                .all(|anchor| locants.contains_key(anchor) && mol.atoms[anchor].symbol == "C");
        let omit =
            available == anchors.len() || (symbol != "H" && available == 1) || only_carbon_parent;
        let mut positions = if omit {
            vec![]
        } else {
            anchors
                .iter()
                .map(|&a| site_locant(mol, locants, a))
                .collect::<Result<Vec<_>, _>>()?
        };
        positions.sort_by_key(|s| retained_locant_sort_key(s));
        let position = if positions.is_empty() {
            String::new()
        } else {
            format!("{}-", positions.join(","))
        };
        let nuclide = if style == IsotopeStyle::Boughton && symbol == "H" && mass == 2 {
            "d".into()
        } else {
            format!("{mass}{symbol}")
        };
        let count = if symbol == "H" || anchors.len() > 1 {
            anchors.len().to_string()
        } else {
            String::new()
        };
        descriptors.push(format!("{position}{nuclide}{count}"));
    }
    Ok(match style {
        IsotopeStyle::Iupac => format!("({})", descriptors.join(",")),
        IsotopeStyle::Boughton => format!("-{}", descriptors.join("-")),
    })
}

fn with_descriptor(
    base: &str,
    rendered_parent: &str,
    descriptor: &str,
    style: IsotopeStyle,
) -> Result<String, String> {
    if style == IsotopeStyle::Boughton {
        return Ok(format!("{base}{descriptor}"));
    }
    // P-82.2.1: substituent prefixes precede the descriptor; the engine's
    // rendered parent identifies the boundary without interpreting the name.
    let prefix = base.strip_suffix(rendered_parent).ok_or_else(|| {
        "openclatura: isotope parent boundary is not preserved by ionic naming".to_owned()
    })?;
    let hyphen = if rendered_parent.starts_with(|c: char| c.is_ascii_digit()) {
        "-"
    } else {
        ""
    };
    Ok(format!("{prefix}{descriptor}{hyphen}{rendered_parent}"))
}

#[cfg(test)]
fn verify(smiles: &str, name: &str) -> Result<(), String> {
    let parser = opsin::Parser::new().map_err(|e| e.to_string())?;
    let result = parser.parse(name, &opsin::ParseOptions::strict());
    if result
        .warnings
        .iter()
        .any(|warning| warning.kind == opsin::WarningKind::StereochemistryIgnored)
    {
        return Err(
            "openclatura: isotope naming could not preserve the specified CIP stereochemistry"
                .into(),
        );
    }
    let structure = result.structure.ok_or_else(|| {
        format!(
            "openclatura: isotope descriptor placement is not supported by the local parser: {}",
            result.message
        )
    })?;
    let reconstructed = structure.semantic_cxsmiles().map_err(|e| e.to_string())?;
    crate::naming::verify_identity(smiles, &reconstructed)
        .map_err(|_| "openclatura: isotope descriptor does not exactly reconstruct the labelled structure and stereochemistry".into())
}

pub(super) fn name_smiles(smiles: &str, style: IsotopeStyle) -> Result<Name, String> {
    let source = graph_io::read_smiles(smiles)?;
    if source
        .atoms
        .values()
        .any(|atom| atom.radical_electrons != 0)
    {
        return Err(
            "openclatura: combined isotope and radical nomenclature is not supported".into(),
        );
    }
    let (parent, sites) = stripped_parent(&source)?;
    if sites.is_empty() {
        return Err("openclatura: isotope naming requires an isotope".into());
    }
    if parent.atoms.values().any(|atom| atom.symbol == "Po") {
        return Err("openclatura: polonium nomenclature is not validated".into());
    }
    super::validate_parent(&parent)?;
    let atoms = parent.atoms.keys().copied().collect();
    let base = component_namer::name_component_with_atom_locants(&parent, &atoms)?;
    let locants = &base.locants;
    super::assess_parent_name(&parent, &base.text);
    let symmetry = lowest_equivalent_map(&parent, &sites, locants)?;
    let descriptor = isotope_descriptors(&parent, &sites, locants, &symmetry, style)?;
    let text = super::nest_enclosing_marks(&with_descriptor(
        &base.text,
        &base.rendered_parent,
        &descriptor,
        style,
    )?);
    let status = if style == IsotopeStyle::Boughton {
        NameStatus::NotPreferred(NameReason::BoughtonIsotope)
    } else {
        match parent.name_status() {
            status @ (NameStatus::NotPreferred(_) | NameStatus::Unverified(_)) => status,
            _ => NameStatus::Unverified(NameReason::IsotopePlacement),
        }
    };
    Ok(Name {
        text,
        status,
        alternatives: vec![],
        omitted_stereo: vec![],
        omitted_units: 0,
        verification: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isotopic_names_reconstruct_exact_graphs() {
        for smiles in [
            "[13CH4]",
            "[2H]C",
            "[2H]C([2H])([2H])O",
            "CO[2H]",
            "[2H]C([2H])(C)O",
            "[2H]C([2H])([2H])O[2H]",
            "C[13CH2]CO",
            "CC[13CH2]C",
            "[15NH2]C",
            "C[18OH]",
            "[2H][C@H](O)C",
            "[2H][C@@H](O)C",
        ] {
            for style in [IsotopeStyle::Iupac, IsotopeStyle::Boughton] {
                let name = name_smiles(smiles, style)
                    .unwrap_or_else(|e| panic!("{smiles} {style:?}: {e}"));
                verify(smiles, &name.text).unwrap();
                assert_eq!(
                    name.status,
                    if style == IsotopeStyle::Boughton {
                        NameStatus::NotPreferred(NameReason::BoughtonIsotope)
                    } else {
                        NameStatus::Unverified(NameReason::IsotopePlacement)
                    }
                );
            }
        }
    }

    #[test]
    fn nuclides_are_ordered_by_symbol_then_increasing_mass() {
        for (smiles, expected) in [
            ("[13CH3]C([2H])([2H])C", "(1-13C,2,2-2H2)propane"),
            ("[2H]C([3H])CO", "(2-2H1,2-3H1)ethan-1-ol"),
            ("C[13CH2]C([2H])C", "(2-13C,3-2H1)butane"),
            ("CC([2H])C([3H])C", "(3-2H1,2-3H1)butane"),
        ] {
            let name = name_smiles(smiles, IsotopeStyle::Iupac).unwrap();
            assert_eq!(name.text, expected);
            verify(smiles, &name.text).unwrap();
            assert!(name_smiles(smiles, IsotopeStyle::Boughton).is_err());
        }
    }

    #[test]
    fn engine_locants_and_equivalent_numbering_ignore_atom_order() {
        for (a, b) in [
            ("CC[13CH2]C", "C[13CH2]CC"),
            ("[2H]CCC", "CCC[2H]"),
            ("[13CH3]CO", "OC[13CH3]"),
        ] {
            let a = name_smiles(a, IsotopeStyle::Iupac).unwrap().text;
            let b = name_smiles(b, IsotopeStyle::Iupac).unwrap().text;
            assert_eq!(a, b);
        }
        assert_eq!(
            name_smiles("CC[13CH2]C", IsotopeStyle::Iupac).unwrap().text,
            "(2-13C)butane"
        );
        assert_eq!(
            name_smiles("[13CH3]CO", IsotopeStyle::Iupac).unwrap().text,
            "(2-13C)ethan-1-ol"
        );
        assert_eq!(
            name_smiles("[2H]C([2H])([2H])O", IsotopeStyle::Iupac)
                .unwrap()
                .text,
            "(2H3)methanol"
        );
        assert_eq!(
            name_smiles("CO[2H]", IsotopeStyle::Iupac).unwrap().text,
            "(O-2H1)methanol"
        );
        assert_eq!(
            name_smiles("[2H]C([2H])(C)O", IsotopeStyle::Boughton)
                .unwrap()
                .text,
            "ethan-1-ol-1,1-d2"
        );
    }

    #[test]
    fn isotope_descriptors_follow_substituents_and_stereodescriptors() {
        for (smiles, expected) in [
            ("[13CH3]CCl", "1-chloro(2-13C)ethane"),
            ("ClC([2H])([2H])Cl", "dichloro(2H2)methane"),
            ("[13CH3][C@@H](F)Cl", "(1S)-1-chloro-1-fluoro(2-13C)ethane"),
            // P-91.3: the only stereocentre of an unlocanted name.
            ("F[13C@H](Cl)Br", "(R)-bromochlorofluoro(13C)methane"),
        ] {
            let name = name_smiles(smiles, IsotopeStyle::Iupac).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            verify(smiles, &name.text).unwrap();
        }
    }

    #[test]
    fn ambiguous_branch_and_suffix_positions_are_refused() {
        for smiles in ["[13CH3]C1CCCCC1", "C(=[18O])O", "[2H][2H]", "[13CH3]"] {
            // Generation keeps only names that the parser reconstructs exactly.
            assert!(
                name_smiles(smiles, IsotopeStyle::Iupac)
                    .and_then(|name| verify(smiles, &name.text))
                    .is_err(),
                "{smiles}"
            );
        }
    }

    #[test]
    fn isotopic_spiro_parent_uses_the_engine_locants_and_exact_graph() {
        let smiles = "[13CH2]1CCC2(CC1)CCCC2";
        let name = name_smiles(smiles, IsotopeStyle::Iupac).unwrap();
        assert!(name.text.contains("13C"));
        verify(smiles, &name.text).unwrap();
        assert!(!matches!(name.status, NameStatus::Preferred));
    }
}
