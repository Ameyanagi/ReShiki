//! Plain-name rendering for heteroatom-starting recursive substituents.
use super::assembly_prefixes::substituent_sort_key;
use super::chains::get_cyclic_atoms;
use super::formatting::{
    format_center_ligands_by, format_counted_prefixes, format_element_substituent,
    format_multiplier, is_complex_prefix, oxy_prefix_from_branch, strip_outer_parentheses,
};
use super::molecule::{AtomSet, Molecule};
use super::namer::{capped_acid_name, name_subgraph_with_upstream};
use super::nomenclature::section;
use super::rules::{elements, multipliers};
use super::subgraph_tools::subgraph_component;
use crate::naming::{NameReason, NameStatus};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::sync::LazyLock;

#[expect(
    clippy::expect_used,
    reason = "the embedded, checked-in substituent table is validated during development"
)]
static SPECS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("data/heteroatom_substituents.json"))
        .expect("heteroatom substituent data")
});
fn unsupported(symbol: &str) -> String {
    format!("openclatura: {symbol} substituent role not ported")
}
fn with_atoms(exclude: &AtomSet, atoms: impl IntoIterator<Item = usize>) -> AtomSet {
    let mut out = exclude.clone();
    out.extend(atoms);
    out
}
pub fn upstream_bond_order(mol: &Molecule, start: usize, upstream: Option<usize>) -> usize {
    upstream.map_or(0, |up| mol.bond_order(start, up))
}
pub fn subgraph_neighbors(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Vec<usize> {
    mol.get_neighbors(start)
        .iter()
        .copied()
        .filter(|n| !exclude.contains(n) && Some(*n) != upstream && mol.atoms[n].symbol != "H")
        .collect()
}
pub fn double_bonded_neighbors(mol: &Molecule, start: usize, symbol: &str) -> Vec<usize> {
    mol.get_neighbors(start)
        .iter()
        .copied()
        .filter(|n| {
            mol.atoms[n].symbol == symbol && mol.get_bond(start, *n).is_some_and(|b| b.order == 2)
        })
        .collect()
}
fn plain_branch(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: usize,
) -> Result<String, String> {
    name_subgraph_with_upstream(mol, start, exclude, Some(upstream))
        .map(|s| strip_outer_parentheses(&s))
}
fn branches(
    mol: &Molecule,
    roots: &[usize],
    exclude: &AtomSet,
    upstream: usize,
) -> Result<Vec<String>, String> {
    roots
        .iter()
        .map(|&n| name_subgraph_with_upstream(mol, n, exclude, Some(upstream)))
        .collect()
}
fn center(names: &[String]) -> String {
    format_center_ligands_by(names, substituent_sort_key)
}
fn stereo(mol: &Molecule, idx: usize) -> String {
    mol.atoms[&idx]
        .stereo
        .as_ref()
        .map(|s| format!("({s})-"))
        .unwrap_or_default()
}
fn prefix(symbol: &str, oxo: usize) -> Option<String> {
    SPECS["heteroatom_substituents"][symbol]["unsubstituted_prefixes"][oxo.to_string()]
        .as_str()
        .map(str::to_owned)
}
fn charged_prefix(symbol: &str, charge: i32, order: usize) -> Option<String> {
    let kind = match order {
        2 => "double",
        3 => "triple",
        _ => "single",
    };
    section("heteroatom_charge_prefixes")
        [format!("{symbol}:{}:{kind}", if charge > 0 { "+" } else { "-" })]
    .as_str()
    .map(str::to_owned)
}
fn has_balanced_oxo_charges(mol: &Molecule, center: usize) -> bool {
    let atom = &mol.atoms[&center];
    atom.charge > 0
        && atom.total_h_count == 0
        && mol
            .get_neighbors(center)
            .iter()
            .filter(|&&n| {
                matches!(mol.atoms[&n].symbol.as_str(), "O" | "S" | "Se" | "Te")
                    && charge_normalized_chalcogenido(mol, Some(center), n)
            })
            .count() as i32
            == atom.charge
}
pub fn substituent_bonding_number(mol: &Molecule, idx: usize) -> usize {
    let atom = &mol.atoms[&idx];
    let sulfur_ylide = atom.symbol == "S"
        && atom.charge > 0
        && mol.get_neighbors(idx).iter().any(|n| {
            mol.atoms[n].is_carbon()
                && mol.atoms[n].charge < 0
                && mol.get_bond(idx, *n).is_some_and(|b| b.order == 2)
        });
    mol.get_neighbors(idx)
        .iter()
        .filter_map(|&n| mol.get_bond(idx, n))
        .map(|b| b.order)
        .sum::<usize>()
        + if sulfur_ylide {
            0
        } else if atom.total_h_count > 0 {
            atom.total_h_count
        } else {
            atom.explicit_h_count
        }
}
fn lambda(mol: &Molecule, idx: usize, names: &[String], suffix: &str) -> String {
    format!(
        "({}{}-lambda^{}-{suffix})",
        stereo(mol, idx),
        center(names),
        substituent_bonding_number(mol, idx)
    )
}

pub fn format_amino_from_branches(
    names: &[String],
    double: bool,
    cation: bool,
    anion: bool,
) -> String {
    if double {
        let suffix = if cation { "iminio" } else { "imino" };
        if names.is_empty() {
            return suffix.into();
        }
        if cation && names.len() > 1 {
            return format!(
                "({}{suffix})",
                center(
                    &names
                        .iter()
                        .map(|n| strip_outer_parentheses(n))
                        .collect::<Vec<_>>()
                )
            );
        }
        if names.len() == 1 {
            let n = strip_outer_parentheses(&names[0]);
            return if is_complex_prefix(&n) {
                format!("(({n}){suffix})")
            } else {
                format!("({n}{suffix})")
            };
        }
        return suffix.into();
    }
    if cation {
        if names.is_empty() {
            return "ammonio".into();
        }
        return format!(
            "({}ammonio)",
            center(
                &names
                    .iter()
                    .map(|n| strip_outer_parentheses(n))
                    .collect::<Vec<_>>()
            )
        );
    }
    if anion {
        return if names.is_empty() {
            "azanidyl".into()
        } else {
            format!(
                "({}azanidyl)",
                center(
                    &names
                        .iter()
                        .map(|n| strip_outer_parentheses(n))
                        .collect::<Vec<_>>()
                )
            )
        };
    }
    if names.len() == 1 {
        let n = strip_outer_parentheses(&names[0]);
        if ["carbonyl", "sulfonyl", "sulfinyl", "carbonothioyl"]
            .iter()
            .any(|s| n.ends_with(s))
            && !n.starts_with('(')
        {
            return format!("{n}amino");
        }
        return if is_complex_prefix(&n) {
            format!("(({n})amino)")
        } else {
            format!("({n}amino)")
        };
    }
    let plain: Vec<_> = names.iter().map(|n| strip_outer_parentheses(n)).collect();
    let modifier = if plain.len() > 1 && plain.iter().all(|n| n == "formyl") {
        format!(
            "{}-{}",
            vec!["N"; plain.len()].join(","),
            format_counted_prefixes(&plain)
        )
    } else {
        center(names)
    };
    format!("({modifier}amino)")
}

/// P-65.3.2.2.2: R-SO2- and R-SO- are named from the acid R-SO3H or R-SO2H by
/// changing "ic acid" to "yl", as in methanesulfonyl or 4-methylbenzene-1-sulfonyl,
/// and enclosed although simple (P-16.5.1.4). A ring nitrogen takes the suffix
/// too (pyrrolidine-1-sulfonamide, P-66.1.1.2), so its substituent name only
/// regains the elided final e: piperidin-1-yl gives piperidine-1-sulfonyl.
/// `outside` holds every atom beyond the sulfur's upstream bond.
fn acid_sulfonyl(
    mol: &Molecule,
    sulfur: usize,
    root: usize,
    branch: &str,
    outside: &AtomSet,
    suffix: &str,
) -> Result<Option<String>, String> {
    let Some(acid) = suffix
        .strip_suffix("yl")
        .filter(|s| matches!(*s, "sulfon" | "sulfin"))
    else {
        return Ok(None);
    };
    let atom = &mol.atoms[&root];
    let acyl = if atom.symbol == "N"
        && atom.charge == 0
        && get_cyclic_atoms(mol, outside).contains(&root)
    {
        branch
            .strip_suffix("-yl")
            .and_then(|b| b.rsplit_once('-'))
            .map(|(parent, locant)| format!("{parent}e-{locant}-{suffix}"))
    } else if atom.is_carbon() {
        let atoms = subgraph_component(mol, sulfur, outside);
        // Another sulfur acid group could take the suffix in the acid's name.
        if atoms.iter().any(|&s| {
            s != sulfur
                && mol.atoms[&s].symbol == "S"
                && mol.get_neighbors(s).iter().any(|&o| {
                    mol.atoms[&o].symbol == "O" && mol.degree(o) == 1 && mol.bond_order(s, o) == 1
                })
        }) {
            return Ok(None);
        }
        mol.candidate(|| {
            Ok(capped_acid_name(mol, sulfur, &atoms)?.and_then(|name| {
                name.strip_suffix(&format!("{acid}ic acid"))
                    .map(|stem| format!("{stem}{suffix}"))
            }))
        })?
    } else {
        None
    };
    Ok(acyl.map(|acyl| format!("({}{acyl})", stereo(mol, sulfur))))
}

fn sulfonyl_group(mol: &Molecule, branch: &str, suffix: &str) -> (String, bool) {
    let b = strip_outer_parentheses(branch);
    if let Some(rows) = section("retained_sulfonyl_group_names").as_array() {
        for row in rows {
            if row["branch"].as_str() == Some(&b)
                && row["suffix"].as_str() == Some(suffix)
                && let Some(name) = row["name"].as_str()
            {
                if name == "tosyl" {
                    mol.mark_name(NameStatus::NotPreferred(NameReason::SulfonylPrefix));
                }
                return (name.into(), true);
            }
        }
    }
    if let Some(rows) = section("retained_sulfonyl_ligand_contractions").as_array() {
        for row in rows {
            if let (Some(ending), Some(s), Some(name)) = (
                row["ligand_ending"].as_str(),
                row["suffix"].as_str(),
                row["name"].as_str(),
            ) && s == suffix
                && let Some(before) = b.strip_suffix(ending)
            {
                let result = format!("{before}{name}");
                return (result, before.is_empty());
            }
        }
    }
    // Preferred names cite methanesulfonyl, sulfo, sulfamoyl (P-65.3.2.3);
    // alkoxy, halo, azido and similar concatenations are preferred prefixes.
    if (b.ends_with("yl") || b.ends_with("amino") || b == "hydroxy")
        && matches!(suffix, "sulfonyl" | "sulfinyl")
    {
        mol.mark_name(NameStatus::NotPreferred(NameReason::SulfonylPrefix));
    }
    (format!("{b}{suffix}"), false)
}

struct CarbonylFragmentStyle<'a> {
    branch_suffix: &'a str,
    fallback: &'a str,
    amino_base: Option<&'a str>,
    wrap: bool,
}

fn carbonyl_like_fragment(
    mol: &Molecule,
    center: usize,
    attach: usize,
    double_atoms: &[usize],
    exclude: &AtomSet,
    style: CarbonylFragmentStyle<'_>,
) -> Result<String, String> {
    let CarbonylFragmentStyle {
        branch_suffix,
        fallback,
        amino_base,
        wrap,
    } = style;
    let next = mol
        .get_neighbors(center)
        .iter()
        .copied()
        .filter(|n| *n != attach && !double_atoms.contains(n))
        .find(|n| mol.atoms[n].is_carbon())
        .or_else(|| {
            mol.get_neighbors(center)
                .iter()
                .copied()
                .find(|n| *n != attach && !double_atoms.contains(n))
        });
    let Some(next) = next else {
        return Ok(fallback.into());
    };
    let blocked = with_atoms(
        exclude,
        [attach, center]
            .into_iter()
            .chain(double_atoms.iter().copied()),
    );
    let name = plain_branch(mol, next, &blocked, center)?;
    if name.is_empty() {
        return Ok(fallback.into());
    }
    if let Some(base) = amino_base
        && let Some(before) = name.strip_suffix("amino")
    {
        return Ok(format!("({before}{base})"));
    }
    Ok(if wrap {
        format!("({name}{branch_suffix})")
    } else {
        format!("{name}{branch_suffix}")
    })
}

fn is_sulfonamido_oxygen_ligand(mol: &Molecule, oxygen: usize, nitrogen: usize) -> bool {
    let atom = &mol.atoms[&nitrogen];
    if mol.atoms[&oxygen].charge != 0
        || atom.symbol != "N"
        || atom.charge != 0
        || atom.is_aromatic
        || atom.total_h_count != 1
        || mol.degree(nitrogen) != 2
        || mol
            .get_bond(oxygen, nitrogen)
            .is_none_or(|bond| bond.order != 1)
    {
        return false;
    }
    let Some(&sulfur) = mol.get_neighbors(nitrogen).iter().find(|&&n| n != oxygen) else {
        return false;
    };
    let center = &mol.atoms[&sulfur];
    if center.symbol != "S"
        || center.charge != 0
        || center.is_aromatic
        || center.total_h_count != 0
        || mol.degree(sulfur) != 4
        || mol
            .get_bond(nitrogen, sulfur)
            .is_none_or(|bond| bond.order != 1)
    {
        return false;
    }
    let oxo = double_bonded_neighbors(mol, sulfur, "O");
    if oxo.len() != 2
        || oxo.iter().any(|&idx| {
            mol.atoms[&idx].charge != 0
                || mol.atoms[&idx].total_h_count != 0
                || mol.degree(idx) != 1
        })
    {
        return false;
    }
    let ligands: Vec<_> = mol
        .get_neighbors(sulfur)
        .iter()
        .copied()
        .filter(|&idx| idx != nitrogen && !oxo.contains(&idx))
        .collect();
    ligands.len() == 1
        && mol.atoms[&ligands[0]].is_carbon()
        && mol
            .get_bond(sulfur, ligands[0])
            .is_some_and(|bond| bond.order == 1)
}

fn name_oxygen(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Result<String, String> {
    let order = upstream_bond_order(mol, start, upstream);
    let roots = subgraph_neighbors(mol, start, exclude, upstream);
    let atom = &mol.atoms[&start];
    // Upstream divergence: oxygen prefixes preserve the complete charge and hydrogen bonding state.
    let expected_bonding = match atom.charge {
        -1 => 1,
        0 => 2,
        1 => 3,
        _ => return Err(unsupported("oxygen charge")),
    };
    if substituent_bonding_number(mol, start) != expected_bonding {
        return Err(unsupported("oxygen bonding state"));
    }
    if atom.charge == 1 && matches!(order, 1 | 2) {
        let suffix = charged_prefix("O", 1, order).ok_or_else(|| unsupported("O"))?;
        let ns = branches(mol, &roots, &with_atoms(exclude, [start]), start)?;
        return Ok(if ns.is_empty() {
            suffix
        } else {
            format!("({}{suffix})", format_counted_prefixes(&ns))
        });
    }
    if roots.is_empty() {
        return Ok(if order == 2 {
            "oxo".into()
        } else if atom.charge == -1 {
            "oxido".into()
        } else {
            "hydroxy".into()
        });
    }
    if roots.len() != 1 {
        return Err(unsupported("O"));
    }
    let next = roots[0];
    let no = double_bonded_neighbors(mol, next, "O");
    let ns = double_bonded_neighbors(mol, next, "S");
    if mol.atoms[&next].symbol == "S" && !no.is_empty() {
        let other: Vec<_> = mol
            .get_neighbors(next)
            .iter()
            .copied()
            .filter(|n| *n != start && !no.contains(n))
            .collect();
        if other.len() <= 1 {
            let suffix = if no.len() == 2 {
                "sulfonyl"
            } else {
                "sulfinyl"
            };
            if let Some(&n) = other.first() {
                let b = plain_branch(
                    mol,
                    n,
                    &with_atoms(exclude, [start, next].into_iter().chain(no)),
                    next,
                )?;
                // HO-SO2-O- is sulfooxy (P-65.3.2.3).
                if b == "hydroxy" && suffix == "sulfonyl" {
                    return Ok("(sulfooxy)".into());
                }
                let outside = with_atoms(exclude, [start]);
                if let Some(acyl) = acid_sulfonyl(mol, next, n, &b, &outside, suffix)? {
                    return Ok(format!("({acyl}oxy)"));
                }
                let (g, _) = sulfonyl_group(mol, &b, suffix);
                return Ok(format!("({}{g}oxy)", stereo(mol, next)));
            }
            return Ok("sulfooxy".into());
        }
    }
    if mol.atoms[&next].is_carbon() && no.len() == 1 {
        let acyl = plain_branch(mol, next, &with_atoms(exclude, [start]), start)?;
        return Ok(if acyl.is_empty() {
            "formyloxy".into()
        } else {
            oxy_prefix_from_branch(mol, &acyl)
        });
    }
    if mol.atoms[&next].is_carbon() && ns.len() == 1 {
        return carbonyl_like_fragment(
            mol,
            next,
            start,
            &ns,
            exclude,
            CarbonylFragmentStyle {
                branch_suffix: "carbonothioyloxy",
                fallback: "methanethioyloxy",
                amino_base: None,
                wrap: true,
            },
        );
    }
    if mol.atoms[&next].symbol == "O" {
        let other: Vec<_> = mol
            .get_neighbors(next)
            .iter()
            .copied()
            .filter(|n| *n != start && mol.atoms[n].symbol != "H")
            .collect();
        if other.len() > 1 {
            return Err(unsupported("peroxy"));
        }
        return Ok(if let Some(&n) = other.first() {
            let b = plain_branch(mol, n, &with_atoms(exclude, [start, next]), next)?;
            format!("({b}peroxy)")
        } else {
            "hydroperoxy".into()
        });
    }
    let b = name_subgraph_with_upstream(mol, next, &with_atoms(exclude, [start]), Some(start))?;
    // Upstream's sulfonic-acid-to-sulfonamido conversion preserves the O-N-S
    // linkage when the named nitrogen bears the proved sulfonyl group.
    // Upstream divergence: other amidooxy contractions change a carbon-acyl aminooxy ligand's parsed connectivity.
    if strip_outer_parentheses(&b).ends_with("amido")
        && !is_complex_prefix(&strip_outer_parentheses(&b))
        && !is_sulfonamido_oxygen_ligand(mol, start, next)
    {
        return Err("openclatura: amidooxy attachment not supported".into());
    }
    Ok(oxy_prefix_from_branch(mol, &b))
}

fn name_nitrogen(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Result<String, String> {
    let order = upstream_bond_order(mol, start, upstream);
    // Upstream divergence: nitrogen charge prefixes require the exact charged bonding state.
    if matches!(mol.atoms[&start].charge, -1 | 1)
        && substituent_bonding_number(mol, start)
            != if mol.atoms[&start].charge > 0 { 4 } else { 2 }
    {
        return Err(unsupported("nitrogen charge bonding state"));
    }
    // Upstream divergence: azo imino-prefix contractions cannot preserve specified N=N geometry.
    if order == 2
        && upstream.is_some_and(|up| {
            mol.atoms[&up].symbol == "N"
                && mol
                    .get_bond(start, up)
                    .is_some_and(|bond| bond.stereo.is_some())
        })
    {
        return Err("openclatura: specified azo imino-prefix geometry is not supported".into());
    }
    let roots = subgraph_neighbors(mol, start, exclude, upstream);
    let atom = &mol.atoms[&start];
    if order == 3
        && atom.charge == 1
        && roots.len() <= 1
        && roots
            .iter()
            .all(|n| mol.get_bond(start, *n).is_some_and(|bond| bond.order == 1))
    {
        let ns = branches(mol, &roots, &with_atoms(exclude, [start]), start)?;
        return Ok(if ns.is_empty() {
            "azaniumylidyne".into()
        } else {
            format!("({}azaniumylidyne)", format_counted_prefixes(&ns))
        });
    }
    if roots.is_empty() {
        return Ok(if order == 3 {
            "nitrilo".into()
        } else if order == 2 {
            charged_prefix("N", atom.charge, 2)
                .filter(|_| atom.charge != 0)
                .unwrap_or_else(|| "imino".into())
        } else if atom.charge > 0 {
            "ammonio".into()
        } else if atom.charge < 0 {
            "azanidyl".into()
        } else {
            "amino".into()
        });
    }
    if order <= 1
        && atom.total_h_count == 0
        && roots.iter().all(|n| {
            mol.atoms[n].symbol == "O" && mol.degree(*n) == 1 && mol.atoms[n].total_h_count == 0
        })
    {
        let orders: Vec<_> = roots
            .iter()
            .map(|n| {
                mol.get_bond(start, *n)
                    .map(|bond| bond.order)
                    .ok_or_else(|| unsupported("nitrogen connectivity"))
            })
            .collect::<Result<_, _>>()?;
        if orders == [2] {
            return Ok("nitroso".into());
        }
        if orders.len() == 2 && (atom.charge == 1 || orders.contains(&2)) {
            return Ok("nitro".into());
        }
    }
    if let Some(name) = terminal_azido(mol, start, exclude, upstream) {
        return Ok(name);
    }
    if let Some(name) = nitrogen_heterocumulene(mol, start, exclude, upstream) {
        return Ok(name);
    }
    if order == 1
        && atom.charge == 0
        && roots.len() == 1
        && upstream.is_some_and(|u| {
            mol.atoms[&u].is_carbon()
                || (mol.atoms[&u].symbol == "N"
                    && mol.get_neighbors(u).iter().any(|n| {
                        *n != start && mol.get_bond(u, *n).is_some_and(|bond| bond.order == 2)
                    }))
        })
    {
        let next = roots[0];
        let bond = mol
            .get_bond(start, next)
            .ok_or_else(|| unsupported("nitrogen connectivity"))?;
        if mol.atoms[&next].symbol == "N" && mol.atoms[&next].charge == 0 && bond.order == 2 {
            let ligands: Vec<_> = mol
                .get_neighbors(next)
                .iter()
                .copied()
                .filter(|n| *n != start && mol.atoms[n].symbol != "H")
                .collect();
            if ligands.len() <= 1
                && ligands
                    .iter()
                    .all(|n| mol.get_bond(next, *n).is_some_and(|bond| bond.order == 1))
            {
                let b = if let Some(&n) = ligands.first() {
                    plain_branch(mol, n, &with_atoms(exclude, [start, next]), next)?
                } else {
                    String::new()
                };
                let modifier = if b.is_empty() {
                    b
                } else {
                    format_multiplier(&b, 1, false)
                };
                let stereo = bond
                    .stereo
                    .as_ref()
                    .filter(|s| matches!(s.as_str(), "E" | "Z"))
                    .map(|s| format!("({s})-"))
                    .unwrap_or_default();
                return Ok(format!("({stereo}{modifier}diazenyl)"));
            }
        }
    }
    let status = mol.name_status();
    if let Some(name) = guanidino_prefix(mol, start, &roots, exclude)? {
        return Ok(name);
    }
    mol.set_name_status(status);
    let mut names = Vec::new();
    for n in roots {
        let oxygen = double_bonded_neighbors(mol, n, "O");
        let sulfur = double_bonded_neighbors(mol, n, "S");
        let b = if mol.atoms[&n].is_carbon() && oxygen.len() == 1 {
            let acyl = plain_branch(mol, n, &with_atoms(exclude, [start]), start)?;
            if acyl.is_empty() {
                "formyl".into()
            } else {
                acyl
            }
        } else if mol.atoms[&n].is_carbon() && sulfur.len() == 1 {
            carbonyl_like_fragment(
                mol,
                n,
                start,
                &sulfur,
                exclude,
                CarbonylFragmentStyle {
                    branch_suffix: "carbonothioyl",
                    fallback: "methanethioyl",
                    amino_base: Some("carbamothioyl"),
                    wrap: false,
                },
            )?
        } else {
            let charge_separated_sulfonyl = mol.degree(n) == 4
                && mol.get_neighbors(n).iter().any(|i| {
                    mol.atoms[i].symbol == "O"
                        && mol.atoms[i].charge == -1
                        && mol.degree(*i) == 1
                        && mol.get_bond(n, *i).is_some_and(|b| b.order == 1)
                });
            if mol.atoms[&n].symbol == "S"
                && !oxygen.is_empty()
                && !charge_separated_sulfonyl
                && let Some(name) = sulfur_imide_branch(mol, start, n, exclude, &oxygen)?
            {
                name
            } else {
                name_subgraph_with_upstream(mol, n, &with_atoms(exclude, [start]), Some(start))?
            }
        };
        if !b.is_empty() {
            names.push(b);
        }
    }
    // The uncharged multiple-imino fallback does not cite its branch names.
    if order == 2 && atom.charge <= 0 && names.len() > 1 {
        mol.set_name_status(status);
    }
    Ok(format_amino_from_branches(
        &names,
        order == 2,
        atom.charge > 0,
        atom.charge < 0,
    ))
}

fn sulfur_imide_branch(
    mol: &Molecule,
    nitrogen: usize,
    sulfur: usize,
    exclude: &AtomSet,
    oxygen: &[usize],
) -> Result<Option<String>, String> {
    let order = mol.bond_order(nitrogen, sulfur);
    if order == 0 {
        return Ok(None);
    }
    let blocked = with_atoms(
        exclude,
        [nitrogen, sulfur].into_iter().chain(oxygen.iter().copied()),
    );
    let roots: Vec<_> = subgraph_neighbors(mol, sulfur, exclude, Some(nitrogen))
        .into_iter()
        .filter(|n| !oxygen.contains(n))
        .collect();
    if order == 2 {
        if let Some(name) = cyclic_sulfur_imide(mol, sulfur, nitrogen, &roots, oxygen) {
            return Ok(Some(name));
        }
        let mut names = branches(mol, &roots, &blocked, sulfur)?;
        names.extend(vec!["oxo".into(); oxygen.len()]);
        return Ok(Some(if names.is_empty() {
            "sulfanylidene".into()
        } else if names.len() == oxygen.len() {
            format!("{}sulfanylidene", format_counted_prefixes(&names))
        } else {
            strip_outer_parentheses(&lambda(mol, sulfur, &names, "sulfanylidene"))
        }));
    }
    let further: Vec<_> = mol
        .get_neighbors(sulfur)
        .iter()
        .copied()
        .filter(|n| *n != nitrogen && !oxygen.contains(n))
        .collect();
    if further.len() > 1 {
        return Ok(None);
    }
    let Some(&next) = further.first() else {
        return Ok(Some("sulfo".into()));
    };
    let name = plain_branch(mol, next, &blocked, sulfur)?;
    let suffix = if oxygen.len() == 2 {
        "sulfonyl"
    } else {
        "sulfinyl"
    };
    let (name, _) = sulfonyl_group(mol, &name, suffix);
    Ok(Some(format!("{}{name}", stereo(mol, sulfur))))
}

fn cyclic_sulfur_imide(
    mol: &Molecule,
    sulfur: usize,
    nitrogen: usize,
    roots: &[usize],
    oxygen: &[usize],
) -> Option<String> {
    if roots.len() != 2 {
        return None;
    }
    let blocked: AtomSet = [sulfur, nitrogen]
        .into_iter()
        .chain(oxygen.iter().copied())
        .collect();
    let mut seen = AtomSet::from([roots[0]]);
    let mut queue = VecDeque::from([(roots[0], vec![roots[0]])]);
    while let Some((current, path)) = queue.pop_front() {
        if current == roots[1] {
            if !path.iter().all(|n| mol.atoms[n].is_carbon()) {
                return None;
            }
            let parent = match path.len() + 1 {
                3 => "thiirane",
                4 => "thietane",
                5 => "thiolane",
                6 => "thiane",
                7 => "thiepane",
                8 => "thiocane",
                _ => return None,
            };
            let prefix = match oxygen.len() {
                0 => String::new(),
                1 => "1-oxo-".into(),
                count => format!("1-{}oxo-", multipliers::basic(count)),
            };
            return Some(format!(
                "{prefix}1lambda^{}-{}-1-ylidene",
                substituent_bonding_number(mol, sulfur),
                parent.trim_end_matches('e')
            ));
        }
        for &next in mol.get_neighbors(current) {
            if blocked.contains(&next)
                || seen.contains(&next)
                || !mol.atoms[&next].is_carbon()
                || mol.get_bond(current, next).is_none_or(|b| b.order != 1)
            {
                continue;
            }
            seen.insert(next);
            let mut next_path = path.clone();
            next_path.push(next);
            queue.push_back((next, next_path));
        }
    }
    None
}

/// Exact oxygen-ligand signatures consumed by the data-shaped class prefixes.
fn central_oxo(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
) -> Result<(Vec<usize>, Option<String>), String> {
    let symbol = &mol.atoms[&start].symbol;
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut oxygen = Vec::new();
    let mut role_classified = true;
    let mut hypervalent_classified =
        matches!(symbol.as_str(), "P" | "S" | "Se" | "Cl" | "Br" | "I");
    for &n in mol.get_neighbors(start) {
        if exclude.contains(&n) || mol.atoms[&n].symbol != "O" {
            continue;
        }
        let a = &mol.atoms[&n];
        let order = mol
            .get_bond(start, n)
            .ok_or_else(|| unsupported("oxo ligand connectivity"))?
            .order;
        let side: Vec<_> = mol
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|o| *o != start)
            .collect();
        let kind = if order == 2
            || (matches!(symbol.as_str(), "Cl" | "Br" | "I")
                && order == 1
                && mol.degree(n) == 1
                && a.charge < 0)
        {
            Some("oxo")
        } else if side.is_empty() {
            Some(if a.charge < 0 { "oxido" } else { "hydroxy" })
        } else if side.len() != 1 || exclude.contains(&side[0]) {
            hypervalent_classified = false;
            None
        } else if mol.atoms[&side[0]].symbol == "O" {
            Some("peroxy")
        } else if mol.atoms[&side[0]].is_carbon() {
            Some("alkoxy")
        } else {
            // Hypervalent roles retain oxygen bonded to another heteroatom as
            // a sigma ligand, rather than consuming it in an oxo class.
            None
        };
        if let Some(kind) = kind {
            *counts.entry(kind).or_default() += 1;
        } else {
            role_classified = false;
        }
        oxygen.push(n);
    }
    let class = if role_classified {
        SPECS["central_oxo_substituent_classes"]
            .as_array()
            .and_then(|rows| {
                rows.iter().find(|row| {
                    row["symbol"].as_str() == Some(symbol)
                        && ["oxo", "oxido", "hydroxy", "alkoxy", "peroxy"]
                            .iter()
                            .all(|k| {
                                row[*k].as_u64().unwrap_or(0) == *counts.get(k).unwrap_or(&0) as u64
                            })
                })
            })
            .and_then(|row| row["prefix"].as_str())
            .map(str::to_owned)
    } else {
        None
    };
    let excluded = if class.is_some() || (role_classified && !hypervalent_classified) {
        oxygen
    } else if hypervalent_classified {
        oxygen
            .into_iter()
            .filter(|n| {
                mol.get_bond(start, *n).is_some_and(|bond| bond.order == 2)
                    || (mol.degree(*n) == 1 && mol.atoms[n].charge < 0)
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok((excluded, class))
}

fn name_sulfur(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Result<String, String> {
    let order = upstream_bond_order(mol, start, upstream);
    let atom = &mol.atoms[&start];
    // Upstream divergence: sulfur charge names require the exact bonding state or complete oxo charge normalization.
    if atom.charge != 0
        && !has_balanced_oxo_charges(mol, start)
        && substituent_bonding_number(mol, start) != if atom.charge > 0 { 3 } else { 1 }
    {
        return Err(unsupported("sulfur charge bonding state"));
    }
    let (oxo, class) = central_oxo(mol, start, exclude)?;
    let nitrogens: Vec<_> = double_bonded_neighbors(mol, start, "N")
        .into_iter()
        .filter(|n| Some(*n) != upstream)
        .collect();
    // Upstream divergence: neutral sulfur oxo/imino and multiple-attachment prefixes cannot cite a residual formal charge.
    if atom.charge != 0
        && !has_balanced_oxo_charges(mol, start)
        && (order != 1 || !oxo.is_empty() || !nitrogens.is_empty())
    {
        return Err(unsupported("charged sulfur oxo/imino attachment"));
    }
    let blocked = with_atoms(
        exclude,
        oxo.iter()
            .copied()
            .chain(nitrogens.iter().copied())
            .chain([start]),
    );
    let roots: Vec<_> = subgraph_neighbors(mol, start, exclude, upstream)
        .into_iter()
        .filter(|n| !oxo.contains(n) && !nitrogens.contains(n))
        .collect();
    let sp = stereo(mol, start);
    let sulfanyl_suffix = if order == 2 {
        "sulfanylidene"
    } else {
        "sulfanyl"
    };
    if oxo.len() == 1 && nitrogens.len() == 1 {
        let suffix = format!("sulfonimidoyl{}", if order == 2 { "idene" } else { "" });
        let n = nitrogens[0];
        let n_roots: Vec<_> = mol
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|i| !blocked.contains(i) && mol.atoms[i].symbol != "H")
            .collect();
        let n_sub = if n_roots.len() == 1 {
            format!(
                "N-{}-",
                format_multiplier(
                    &plain_branch(mol, n_roots[0], &with_atoms(&blocked, [n]), n)?,
                    1,
                    false
                )
            )
        } else {
            String::new()
        };
        return match roots.as_slice() {
            [] => Ok(format!("{sp}{n_sub}{suffix}")),
            [root] => {
                let b = plain_branch(mol, *root, &blocked, start)?;
                Ok(if !b.is_empty() && !n_sub.is_empty() {
                    format!("({sp}{n_sub}S-{b}{suffix})")
                } else if !b.is_empty() {
                    format!("({sp}{b}{suffix})")
                } else {
                    format!("{sp}{suffix}")
                })
            }
            _ => {
                let mut ns = branches(mol, &roots, &blocked, start)?;
                ns.push("oxo".into());
                ns.push("imino".into());
                Ok(lambda(mol, start, &ns, sulfanyl_suffix))
            }
        };
    }
    if !oxo.is_empty() {
        let suffix = format!(
            "{}{}",
            class.unwrap_or_else(|| if oxo.len() == 2 {
                "sulfonyl".into()
            } else {
                "sulfinyl".into()
            }),
            if order == 2 { "idene" } else { "" }
        );
        return match roots.as_slice() {
            [] => Ok(format!("{sp}{suffix}")),
            [root] => {
                let b = plain_branch(mol, *root, &blocked, start)?;
                let outside = with_atoms(exclude, upstream);
                if let Some(acyl) = acid_sulfonyl(mol, start, *root, &b, &outside, &suffix)? {
                    return Ok(acyl);
                }
                let (g, bare) = sulfonyl_group(mol, &b, &suffix);
                Ok(if bare {
                    format!("{sp}{g}")
                } else {
                    format!("({sp}{g})")
                })
            }
            _ => {
                let mut ns = branches(mol, &roots, &blocked, start)?;
                ns.extend(vec!["oxo".into(); oxo.len()]);
                Ok(lambda(mol, start, &ns, sulfanyl_suffix))
            }
        };
    }
    if !nitrogens.is_empty() {
        let mut imino = Vec::new();
        for &nitrogen in &nitrogens {
            let n_roots: Vec<_> = mol
                .get_neighbors(nitrogen)
                .iter()
                .copied()
                .filter(|n| !blocked.contains(n) && mol.atoms[n].symbol != "H")
                .collect();
            let status = mol.name_status();
            let ns = branches(mol, &n_roots, &with_atoms(&blocked, [nitrogen]), nitrogen)?;
            if ns.len() > 1 {
                mol.set_name_status(status);
            }
            imino.push(format_amino_from_branches(&ns, true, false, false));
        }
        if roots.is_empty() {
            return Ok(format!("{sp}{}{sulfanyl_suffix}", center(&imino)));
        }
        let mut ns = branches(mol, &roots, &blocked, start)?;
        imino.append(&mut ns);
        if order != 2 && roots.len() == 1 && nitrogens.len() == 1 {
            return Ok(format!("({sp}{}{sulfanyl_suffix})", center(&imino)));
        }
        return Ok(lambda(mol, start, &imino, sulfanyl_suffix));
    }
    if roots.is_empty() {
        return Ok(
            if order == 2 || charge_normalized_chalcogenido(mol, upstream, start) {
                mol.mark_name(NameStatus::NotPreferred(NameReason::ChalcogenPrefix));
                "thioxo".into()
            } else if mol.atoms[&start].charge > 0 {
                format!("{sp}sulfaniumyl")
            } else if mol.atoms[&start].charge < 0 {
                "sulfido".into()
            } else {
                format!("{sp}sulfanyl")
            },
        );
    }
    let ns = branches(mol, &roots, &blocked, start)?;
    if roots.len() == 1 {
        if mol.atoms[&start].charge > 0 && order != 2 {
            return Ok(format!("({sp}{}sulfaniumyl)", ns[0]));
        }
        // Upstream divergence: a nonstandard sulfur hydride needs its full bonding number in the prefix.
        if mol.atoms[&start].charge == 0
            && mol.atoms[&start].total_h_count > 0
            && substituent_bonding_number(mol, start) != 2
        {
            return Ok(lambda(mol, start, &ns, sulfanyl_suffix));
        }
        if super::nomenclature::RULES
            .heteroatoms
            .simple_sulfanyl_prefixes
            .contains(&ns[0])
        {
            return Ok(format!("({sp}{}sulfanyl)", ns[0]));
        }
        return Ok(format_element_substituent(
            &sp,
            &ns[0],
            "sulfanyl",
            order == 2,
        ));
    }
    if mol.atoms[&start].charge > 0 && order != 2 {
        return Ok(format!("({sp}{}sulfaniumyl)", center(&ns)));
    }
    Ok(lambda(mol, start, &ns, sulfanyl_suffix))
}

pub fn name_heteroatom_subgraph(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Result<Option<String>, String> {
    let symbol = mol.atoms[&start].symbol.as_str();
    let order = upstream_bond_order(mol, start, upstream);
    let name = match symbol {
        "O" => name_oxygen(mol, start, exclude, upstream)?,
        "N" => name_nitrogen(mol, start, exclude, upstream)?,
        "S" => name_sulfur(mol, start, exclude, upstream)?,
        "F" | "Cl" | "Br" | "I" => {
            // Upstream divergence: neutral halogen prefixes require explicit charge-balanced oxo normalization for charged centers.
            if mol.atoms[&start].charge != 0 && !has_balanced_oxo_charges(mol, start) {
                return Err(unsupported("halogen charge"));
            }
            let roots = subgraph_neighbors(mol, start, exclude, upstream);
            if roots.is_empty() {
                section("halogen_prefixes")[symbol]
                    .as_str()
                    .ok_or_else(|| unsupported(symbol))?
                    .into()
            } else {
                let mut oxo: AtomSet = roots
                    .iter()
                    .copied()
                    .filter(|&n| {
                        mol.atoms[&n].symbol == "O"
                            && charge_normalized_chalcogenido(mol, Some(start), n)
                    })
                    .collect();
                if oxo.len() as i32 != mol.atoms[&start].charge {
                    oxo.clear();
                }
                let ns = roots
                    .iter()
                    .map(|&n| {
                        if oxo.contains(&n) {
                            Ok("oxo".into())
                        } else {
                            name_subgraph_with_upstream(
                                mol,
                                n,
                                &with_atoms(exclude, [start]),
                                Some(start),
                            )
                        }
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let valence: usize = mol
                    .get_neighbors(start)
                    .iter()
                    .filter_map(|&n| mol.get_bond(start, n))
                    .map(|b| b.order)
                    .sum::<usize>()
                    + oxo.len();
                let suffix = section("halogen_lambda_suffixes")[symbol]
                    .as_str()
                    .ok_or_else(|| unsupported(symbol))?;
                format!("({}lambda^{valence}-{suffix})", center(&ns))
            }
        }
        "B" | "Si" => {
            // Upstream divergence: neutral boryl/silyl prefixes do not represent charged substituents.
            if mol.atoms[&start].charge != 0 {
                return Err(format!(
                    "openclatura: charged {symbol} substituent is not supported"
                ));
            }
            let suffix = format!(
                "{}{}",
                prefix(symbol, 0).ok_or_else(|| unsupported(symbol))?,
                if order == 2 { "idene" } else { "" }
            );
            let roots = subgraph_neighbors(mol, start, exclude, upstream);
            if roots.is_empty() {
                suffix
            } else {
                format!(
                    "({}{suffix})",
                    center(&branches(
                        mol,
                        &roots,
                        &with_atoms(exclude, [start]),
                        start
                    )?)
                )
            }
        }
        "P" | "As" | "Sb" => {
            let atom = &mol.atoms[&start];
            let charged = if atom.charge != 0 && !has_balanced_oxo_charges(mol, start) {
                // Upstream divergence: charged pnictogen branches need an available prefix for their exact bonding state.
                if atom.charge != 1 || substituent_bonding_number(mol, start) != 4 {
                    return Err(unsupported("pnictogen charge bonding state"));
                }
                Some(
                    charged_prefix(symbol, atom.charge, order)
                        .ok_or_else(|| unsupported("pnictogen charge prefix"))?,
                )
            } else {
                None
            };
            let (oxo, class) = central_oxo(mol, start, exclude)?;
            // Upstream divergence: a charged pnictogen prefix cannot absorb unrendered oxo ligands.
            if charged.is_some() && !oxo.is_empty() {
                return Err(unsupported("charged pnictogen oxo attachment"));
            }
            let roots: Vec<_> = subgraph_neighbors(mol, start, exclude, upstream)
                .into_iter()
                .filter(|n| !oxo.contains(n))
                .collect();
            let deficient = if class.is_none() && roots.len() < 2 {
                SPECS["heteroatom_substituents"][symbol]["ligand_deficient_prefixes"]
                    [oxo.len().to_string()]
                .as_str()
                .map(str::to_owned)
                .or_else(|| {
                    if oxo.is_empty() {
                        None
                    } else {
                        SPECS["heteroatom_substituents"][symbol]["ligand_deficient_prefixes"]["1"]
                            .as_str()
                            .map(str::to_owned)
                    }
                })
            } else {
                None
            };
            let mut suffix = class
                .or(deficient)
                .or_else(|| prefix(symbol, oxo.len()))
                .or_else(|| prefix(symbol, usize::from(!oxo.is_empty())))
                .ok_or_else(|| unsupported(symbol))?;
            suffix.push_str(match order {
                2 => "idene",
                3 => "idyne",
                _ => "",
            });
            if let Some(charged) = charged {
                suffix = charged;
            }
            let sp = stereo(mol, start);
            if roots.is_empty() {
                format!("{sp}{suffix}")
            } else {
                let ns = branches(
                    mol,
                    &roots,
                    &with_atoms(exclude, [start].into_iter().chain(oxo.iter().copied())),
                    start,
                )?;
                let atom = &mol.atoms[&start];
                let balanced = mol
                    .get_neighbors(start)
                    .iter()
                    .any(|&n| charge_normalized_chalcogenido(mol, Some(start), n));
                if atom.charge != 0
                    && !balanced
                    && let Some(charged) = charged_prefix(symbol, atom.charge, order)
                {
                    suffix = charged;
                }
                if atom.charge == 0
                    && oxo.is_empty()
                    && mol
                        .get_neighbors(start)
                        .iter()
                        .all(|n| mol.get_bond(start, *n).is_some_and(|bond| bond.order == 1))
                    && substituent_bonding_number(mol, start)
                        > elements::get(symbol).map_or(3, |e| e.standard_valence)
                {
                    lambda(mol, start, &ns, &suffix)
                } else {
                    format!("({sp}{}{suffix})", center(&ns))
                }
            }
        }
        "Se" | "Te" => {
            let atom = &mol.atoms[&start];
            // Upstream divergence: selanyl/tellanyl prefixes preserve only terminal unit anions or fully normalized oxo charges.
            if atom.charge != 0
                && !has_balanced_oxo_charges(mol, start)
                && !(atom.charge == -1
                    && order == 1
                    && mol.degree(start) == 1
                    && substituent_bonding_number(mol, start) == 1)
            {
                return Err(unsupported("chalcogen charge bonding state"));
            }
            let (oxo, class) = central_oxo(mol, start, exclude)?;
            let suffix = prefix(symbol, 0).ok_or_else(|| unsupported(symbol))?;
            let roots: Vec<_> = subgraph_neighbors(mol, start, exclude, upstream)
                .into_iter()
                .filter(|n| !oxo.contains(n))
                .collect();
            let sp = stereo(mol, start);
            if !oxo.is_empty() {
                if roots.is_empty() {
                    format!(
                        "{sp}{}",
                        class
                            .or_else(|| prefix(symbol, oxo.len()))
                            .unwrap_or(suffix)
                    )
                } else {
                    let mut ns = branches(
                        mol,
                        &roots,
                        &with_atoms(exclude, [start].into_iter().chain(oxo.iter().copied())),
                        start,
                    )?;
                    ns.extend(vec!["oxo".into(); oxo.len()]);
                    lambda(
                        mol,
                        start,
                        &ns,
                        &format!("{suffix}{}", if order == 2 { "idene" } else { "" }),
                    )
                }
            } else if roots.is_empty() {
                if order == 2 {
                    mol.mark_name(NameStatus::NotPreferred(NameReason::ChalcogenPrefix));
                    if symbol == "Se" {
                        "selenoxo".into()
                    } else {
                        "telluroxo".into()
                    }
                } else if mol.atoms[&start].charge < 0 {
                    format!(
                        "{sp}{}",
                        if symbol == "Se" {
                            "selenido"
                        } else {
                            "tellurido"
                        }
                    )
                } else {
                    format!("{sp}{suffix}")
                }
            } else {
                let ns = branches(mol, &roots, &with_atoms(exclude, [start]), start)?;
                if ns.len() == 1
                    && substituent_bonding_number(mol, start)
                        == elements::get(symbol).map_or(2, |e| e.standard_valence)
                {
                    if symbol == "Se"
                        && super::nomenclature::RULES
                            .heteroatoms
                            .simple_selanyl_prefixes
                            .contains(&ns[0])
                    {
                        format!("({sp}{}{suffix})", ns[0])
                    } else {
                        format_element_substituent(&sp, &ns[0], &suffix, order == 2)
                    }
                } else {
                    lambda(
                        mol,
                        start,
                        &ns,
                        &format!("{suffix}{}", if order == 2 { "idene" } else { "" }),
                    )
                }
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(name))
}

fn terminal_azido(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Option<String> {
    let upstream = upstream?;
    let order = mol.get_bond(start, upstream)?.order;
    let middle: Vec<_> = mol
        .get_neighbors(start)
        .iter()
        .copied()
        .filter(|n| *n != upstream && !exclude.contains(n) && mol.atoms[n].symbol == "N")
        .collect();
    if middle.len() != 1 {
        return None;
    }
    let mid = middle[0];
    let tail: Vec<_> = mol
        .get_neighbors(mid)
        .iter()
        .copied()
        .filter(|n| *n != start && !exclude.contains(n) && mol.atoms[n].symbol == "N")
        .collect();
    if tail.len() != 1
        || mol
            .get_neighbors(tail[0])
            .iter()
            .any(|n| *n != mid && mol.atoms[n].symbol != "H")
    {
        return None;
    }
    if mol.get_bond(start, mid)?.order != 2 || mol.get_bond(mid, tail[0])?.order != 2 {
        return None;
    }
    let charges = (
        mol.atoms[&start].charge,
        mol.atoms[&mid].charge,
        mol.atoms[&tail[0]].charge,
    );
    if (order == 1 && charges == (0, 0, 0))
        || (matches!(order, 1 | 2) && matches!(charges, (0, 1, -1) | (-1, 1, 0)))
    {
        Some("azido".into())
    } else {
        None
    }
}

fn nitrogen_heterocumulene(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Option<String> {
    if upstream_bond_order(mol, start, upstream) != 1 {
        return None;
    }
    let mut candidates = Vec::new();
    for &center in mol.get_neighbors(start) {
        if Some(center) == upstream
            || exclude.contains(&center)
            || !mol.atoms[&center].is_carbon()
            || mol.get_bond(start, center)?.order != 2
        {
            continue;
        }
        let terminal: Vec<_> = mol
            .get_neighbors(center)
            .iter()
            .copied()
            .filter(|n| *n != start && !exclude.contains(n))
            .collect();
        if terminal.len() == 1
            && matches!(mol.atoms[&terminal[0]].symbol.as_str(), "O" | "S" | "Se")
            && mol.get_bond(center, terminal[0])?.order == 2
        {
            candidates.push(terminal[0]);
        }
    }
    if candidates.len() != 1 {
        return None;
    }
    Some(
        match mol.atoms[&candidates[0]].symbol.as_str() {
            "O" => "isocyanato",
            "S" => "isothiocyanato",
            "Se" => "isoselenocyanato",
            _ => return None,
        }
        .into(),
    )
}

fn guanidino_prefix(
    mol: &Molecule,
    start: usize,
    roots: &[usize],
    exclude: &AtomSet,
) -> Result<Option<String>, String> {
    if mol.atoms[&start].charge != 0 {
        return Ok(None);
    }
    let carbons: Vec<_> = roots
        .iter()
        .copied()
        .filter(|&n| {
            mol.atoms[&n].is_carbon()
                && mol.degree(n) == 3
                && mol.get_bond(start, n).is_some_and(|b| b.order == 1)
                && mol
                    .get_neighbors(n)
                    .iter()
                    .all(|x| mol.atoms[x].symbol == "N")
        })
        .collect();
    if carbons.len() != 1 {
        return Ok(None);
    }
    let carbon = carbons[0];
    let ns: Vec<_> = mol
        .get_neighbors(carbon)
        .iter()
        .copied()
        .filter(|n| *n != start)
        .collect();
    if ns.len() != 2 || ns.iter().any(|n| mol.atoms[n].charge != 0) {
        return Ok(None);
    }
    let cyclic = super::chains::get_cyclic_atoms(mol, &AtomSet::new());
    if [carbon, start]
        .iter()
        .chain(ns.iter())
        .any(|n| cyclic.contains(n))
    {
        return Ok(None);
    }
    let imino = ns
        .iter()
        .copied()
        .find(|&n| mol.get_bond(carbon, n).is_some_and(|b| b.order == 2));
    let amino = ns
        .iter()
        .copied()
        .find(|&n| mol.get_bond(carbon, n).is_some_and(|b| b.order == 1));
    let (Some(imino), Some(amino)) = (imino, amino) else {
        return Ok(None);
    };
    if exclude.contains(&imino) || exclude.contains(&amino) {
        return Ok(None);
    }
    for nitrogen in [start, imino, amino] {
        for &n in mol.get_neighbors(nitrogen) {
            if n != carbon
                && ((!mol.atoms[&n].is_carbon() && mol.atoms[&n].symbol != "H")
                    || mol.get_bond(nitrogen, n).is_none_or(|b| b.order != 1))
            {
                return Ok(None);
            }
        }
    }
    let blocked = with_atoms(exclude, [start, carbon, imino, amino]);
    let mut names = BTreeMap::<String, Vec<String>>::new();
    for (nitrogen, locant) in [(start, "1"), (imino, "2"), (amino, "3")] {
        for &n in mol.get_neighbors(nitrogen) {
            if n == carbon
                || exclude.contains(&n)
                || mol.atoms[&n].symbol == "H"
                || (nitrogen == start && !roots.contains(&n))
            {
                continue;
            }
            let name = name_subgraph_with_upstream(mol, n, &blocked, Some(nitrogen))?;
            if name.is_empty() {
                return Ok(None);
            }
            names.entry(name).or_default().push(locant.into());
        }
    }
    if names.is_empty() {
        mol.mark_name(NameStatus::NotPreferred(NameReason::AminoPrefix));
        return Ok(Some("guanidino".into()));
    }
    let mut ordered: Vec<_> = names.into_iter().collect();
    ordered.sort_by_key(|(name, _)| substituent_sort_key(name));
    let prefixes = ordered
        .into_iter()
        .map(|(name, locants)| {
            format!(
                "{}-{}",
                locants.join(","),
                format_multiplier(&name, locants.len(), is_complex_prefix(&name))
            )
        })
        .collect::<Vec<_>>()
        .join("-");
    let stereo = mol
        .get_bond(carbon, imino)
        .and_then(|b| b.stereo.as_ref())
        .filter(|s| matches!(s.as_str(), "E" | "Z"))
        .map(|s| format!("({s})-"))
        .unwrap_or_default();
    mol.mark_name(NameStatus::NotPreferred(NameReason::AminoPrefix));
    Ok(Some(format!("({stereo}{prefixes}guanidino)")))
}

fn charge_normalized_chalcogenido(mol: &Molecule, center: Option<usize>, ligand: usize) -> bool {
    let Some(center) = center else {
        return false;
    };
    mol.atoms[&center].charge > 0
        && matches!(
            mol.atoms[&center].symbol.as_str(),
            "P" | "S" | "Se" | "Te" | "Cl" | "Br" | "I"
        )
        && mol.get_bond(center, ligand).is_some_and(|b| b.order == 1)
        && mol.atoms[&ligand].charge < 0
        && mol.degree(ligand) == 1
        && mol.atoms[&ligand].total_h_count == 0
}

#[cfg(test)]
mod tests {
    use super::central_oxo;
    use crate::naming::NameStatus;
    use crate::naming::openclatura::{
        graph_io::read_smiles, molecule::AtomSet, namer::name_subgraph_with_upstream,
    };

    #[test]
    fn amino_contractions_discard_the_consumed_phenylamino_status() -> Result<(), String> {
        for (smiles, start, upstream, expected) in [
            ("CCS(=O)(=O)Nc1ccccc1", 2, 1, "(phenylsulfamoyl)"),
            ("CCNC(=S)Nc1ccccc1", 2, 1, "(phenylcarbamothioylamino)"),
        ] {
            let mol = read_smiles(smiles)?;
            assert_eq!(
                name_subgraph_with_upstream(&mol, start, &AtomSet::from([0, 1]), Some(upstream))?,
                expected,
                "{smiles}"
            );
            assert_eq!(mol.name_status(), NameStatus::Preferred, "{smiles}");
        }
        Ok(())
    }

    #[test]
    fn sulfonamidooxy_keeps_the_proved_oxygen_nitrogen_sulfur_linkage() -> Result<(), String> {
        let mol = read_smiles("CONS(=O)(=O)c1ccccc1")?;
        assert_eq!(
            name_subgraph_with_upstream(&mol, 1, &AtomSet::from([0]), Some(0))?,
            "(benzenesulfonamidooxy)"
        );
        let carbon_amide = read_smiles("CONC(C)=O")?;
        assert!(
            name_subgraph_with_upstream(&carbon_amide, 1, &AtomSet::from([0]), Some(0)).is_err()
        );
        assert_eq!(
            crate::naming::openclatura::name_smiles(
                "CCC(C1=NOC2=C1C(=C(C=C2)Br)OC)ONS(=O)(=O)C3=CC=CC=C3"
            )?
            .text,
            "3-[1-(benzenesulfonamidooxy)propyl]-5-bromo-4-methoxy-1,2-benzoxazole"
        );
        Ok(())
    }

    #[test]
    fn heteroatom_branches_preserve_acyl_ligands_and_oxo_roles() -> Result<(), String> {
        for (smiles, expected) in [
            ("CNC(=S)CC(C)C", "2-methylpropylcarbonothioylamino"),
            ("CNC(=S)N(C)C", "((dimethylcarbamothioyl)amino)"),
            ("CN(C(=O)OC)C", "(methoxycarbonyl(methyl)amino)"),
            ("COC(=S)N(C)C", "(dimethylaminocarbonothioyloxy)"),
            ("CP(=O)(O)ON", "((aminooxy)(hydroxy)phosphoryl)"),
            ("CS(=O)(=O)ON=CC", "(ethylideneaminooxysulfonyl)"),
            (
                "CN=S(=O)(C)C",
                "((dimethyl(oxo)-lambda^6-sulfanylidene)amino)",
            ),
            (
                "CN=S1(=O)CCCC1",
                "((1-oxo-1lambda^6-thiolan-1-ylidene)amino)",
            ),
        ] {
            let mol = read_smiles(smiles)?;
            assert_eq!(
                name_subgraph_with_upstream(&mol, 1, &AtomSet::from([0]), Some(0))?,
                expected,
                "{smiles}"
            );
        }
        Ok(())
    }

    #[test]
    fn generic_oxygen_ligands_stay_outside_exact_oxo_classes() -> Result<(), String> {
        for smiles in ["CP(=O)(O)ON", "CP(=O)(O)OP(=O)(O)O", "CP(=O)(O)OBr"] {
            let mol = read_smiles(smiles)?;
            let (oxygen, class) = central_oxo(&mol, 1, &AtomSet::from([0]))?;
            assert_eq!(oxygen, vec![2], "{smiles}");
            assert_eq!(class, None, "{smiles}");
        }
        Ok(())
    }
}
