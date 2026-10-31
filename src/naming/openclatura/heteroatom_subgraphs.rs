//! Plain-name rendering for heteroatom-starting recursive substituents.
use super::assembly_prefixes::substituent_sort_key;
use super::formatting::{
    format_center_ligands_by, format_counted_prefixes, format_element_substituent,
    format_multiplier, is_complex_prefix, oxy_prefix_from_branch, strip_outer_parentheses,
};
use super::molecule::{AtomSet, Molecule};
use super::namer::name_subgraph_with_upstream;
use super::nomenclature::section;
use super::rules::elements;
use serde_json::Value;
use std::collections::BTreeMap;
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
    upstream
        .and_then(|up| mol.get_bond(start, up))
        .map_or(0, |b| b.order)
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
fn branch(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: usize,
) -> Result<String, String> {
    name_subgraph_with_upstream(mol, start, exclude, Some(upstream))
}
fn plain_branch(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: usize,
) -> Result<String, String> {
    branch(mol, start, exclude, upstream).map(|s| strip_outer_parentheses(&s))
}
fn branches(
    mol: &Molecule,
    roots: &[usize],
    exclude: &AtomSet,
    upstream: usize,
) -> Result<Vec<String>, String> {
    roots
        .iter()
        .map(|&n| branch(mol, n, exclude, upstream))
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
fn charged_prefix(symbol: &str, charge: i32, kind: &str) -> Option<String> {
    section("heteroatom_charge_prefixes")
        [format!("{symbol}:{}:{kind}", if charge > 0 { "+" } else { "-" })]
    .as_str()
    .map(str::to_owned)
}
pub fn substituent_bonding_number(mol: &Molecule, idx: usize) -> usize {
    mol.get_neighbors(idx)
        .iter()
        .filter_map(|&n| mol.get_bond(idx, n))
        .map(|b| b.order)
        .sum::<usize>()
        + mol.atoms[&idx]
            .total_h_count
            .max(mol.atoms[&idx].explicit_h_count)
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

fn sulfonyl_group(branch: &str, suffix: &str) -> (String, bool) {
    let b = strip_outer_parentheses(branch);
    if let Some(rows) = section("retained_sulfonyl_group_names").as_array() {
        for row in rows {
            if row["branch"].as_str() == Some(&b)
                && row["suffix"].as_str() == Some(suffix)
                && let Some(name) = row["name"].as_str()
            {
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
    (format!("{b}{suffix}"), false)
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
    if atom.charge == 1 && matches!(order, 1 | 2) {
        let suffix = charged_prefix("O", 1, if order == 2 { "double" } else { "single" })
            .ok_or_else(|| unsupported("O"))?;
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
                let (g, _) = sulfonyl_group(&b, suffix);
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
            oxy_prefix_from_branch(&acyl)
        });
    }
    if mol.atoms[&next].is_carbon() && ns.len() == 1 {
        let other = mol
            .get_neighbors(next)
            .iter()
            .copied()
            .find(|n| *n != start && !ns.contains(n));
        return Ok(if let Some(n) = other {
            let b = plain_branch(
                mol,
                n,
                &with_atoms(exclude, [start, next].into_iter().chain(ns)),
                next,
            )?;
            format!("({b}carbonothioyloxy)")
        } else {
            "methanethioyloxy".into()
        });
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
    let b = branch(mol, next, &with_atoms(exclude, [start]), start)?;
    Ok(oxy_prefix_from_branch(&b))
}

fn name_nitrogen(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Result<String, String> {
    let order = upstream_bond_order(mol, start, upstream);
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
            charged_prefix("N", atom.charge, "double")
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
    if let Some(name) = guanidino_prefix(mol, start, &roots, exclude)? {
        return Ok(name);
    }
    let mut names = Vec::new();
    for n in roots {
        let b = branch(mol, n, &with_atoms(exclude, [start]), start)?;
        if !b.is_empty() {
            names.push(b);
        }
    }
    Ok(format_amino_from_branches(
        &names,
        order == 2,
        atom.charge > 0,
        atom.charge < 0,
    ))
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
    for &n in mol.get_neighbors(start) {
        if exclude.contains(&n) || mol.atoms[&n].symbol != "O" {
            continue;
        }
        let a = &mol.atoms[&n];
        let order = mol
            .get_bond(start, n)
            .ok_or_else(|| unsupported("oxo ligand connectivity"))?
            .order;
        let kind = if order == 2 {
            "oxo"
        } else if order == 1 && mol.degree(n) == 1 && a.charge < 0 {
            "oxido"
        } else if order == 1 && mol.degree(n) == 1 {
            "hydroxy"
        } else if order == 1
            && mol
                .get_neighbors(n)
                .iter()
                .any(|o| *o != start && mol.atoms[o].symbol == "O")
        {
            "peroxy"
        } else if order == 1
            && mol
                .get_neighbors(n)
                .iter()
                .any(|o| *o != start && mol.atoms[o].is_carbon())
        {
            "alkoxy"
        } else {
            return Err(unsupported(symbol));
        };
        *counts.entry(kind).or_default() += 1;
        oxygen.push(n);
    }
    let class = SPECS["central_oxo_substituent_classes"]
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
        .map(str::to_owned);
    let excluded =
        if class.is_some() || !matches!(symbol.as_str(), "P" | "S" | "Se" | "Cl" | "Br" | "I") {
            oxygen
        } else {
            oxygen
                .into_iter()
                .filter(|n| {
                    mol.get_bond(start, *n).is_some_and(|bond| bond.order == 2)
                        || mol.atoms[n].charge < 0
                })
                .collect()
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
    let (oxo, class) = central_oxo(mol, start, exclude)?;
    let nitrogens: Vec<_> = double_bonded_neighbors(mol, start, "N")
        .into_iter()
        .filter(|n| Some(*n) != upstream)
        .collect();
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
        if roots.is_empty() {
            return Ok(format!("{sp}{n_sub}{suffix}"));
        }
        if roots.len() == 1 {
            let b = plain_branch(mol, roots[0], &blocked, start)?;
            return Ok(if !b.is_empty() && !n_sub.is_empty() {
                format!("({sp}{n_sub}S-{b}{suffix})")
            } else if !b.is_empty() {
                format!("({sp}{b}{suffix})")
            } else {
                format!("{sp}{suffix}")
            });
        }
        let mut ns = branches(mol, &roots, &blocked, start)?;
        ns.push("oxo".into());
        ns.push("imino".into());
        return Ok(lambda(
            mol,
            start,
            &ns,
            if order == 2 {
                "sulfanylidene"
            } else {
                "sulfanyl"
            },
        ));
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
        if roots.is_empty() {
            return Ok(format!("{sp}{suffix}"));
        }
        if roots.len() == 1 {
            let b = plain_branch(mol, roots[0], &blocked, start)?;
            let (g, bare) = sulfonyl_group(&b, &suffix);
            return Ok(if bare {
                format!("{sp}{g}")
            } else {
                format!("({sp}{g})")
            });
        }
        let mut ns = branches(mol, &roots, &blocked, start)?;
        ns.extend(vec!["oxo".into(); oxo.len()]);
        return Ok(lambda(
            mol,
            start,
            &ns,
            if order == 2 {
                "sulfanylidene"
            } else {
                "sulfanyl"
            },
        ));
    }
    if !nitrogens.is_empty() {
        let suffix = if order == 2 {
            "sulfanylidene"
        } else {
            "sulfanyl"
        };
        let mut imino = Vec::new();
        for &nitrogen in &nitrogens {
            let n_roots: Vec<_> = mol
                .get_neighbors(nitrogen)
                .iter()
                .copied()
                .filter(|n| !blocked.contains(n) && mol.atoms[n].symbol != "H")
                .collect();
            let ns = branches(mol, &n_roots, &with_atoms(&blocked, [nitrogen]), nitrogen)?;
            imino.push(format_amino_from_branches(&ns, true, false, false));
        }
        if roots.is_empty() {
            return Ok(format!("{sp}{}{suffix}", center(&imino)));
        }
        let mut ns = branches(mol, &roots, &blocked, start)?;
        imino.append(&mut ns);
        if order != 2 && roots.len() == 1 && nitrogens.len() == 1 {
            return Ok(format!("({sp}{}{suffix})", center(&imino)));
        }
        return Ok(lambda(mol, start, &imino, suffix));
    }
    if roots.is_empty() {
        return Ok(
            if order == 2 || charge_normalized_chalcogenido(mol, upstream, start) {
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
    Ok(lambda(
        mol,
        start,
        &ns,
        if order == 2 {
            "sulfanylidene"
        } else {
            "sulfanyl"
        },
    ))
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
                            branch(mol, n, &with_atoms(exclude, [start]), start)
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
            let (oxo, class) = central_oxo(mol, start, exclude)?;
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
                    && let Some(charged) = charged_prefix(
                        symbol,
                        atom.charge,
                        match order {
                            2 => "double",
                            3 => "triple",
                            _ => "single",
                        },
                    )
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
            let name = branch(mol, n, &blocked, nitrogen)?;
            if name.is_empty() {
                return Ok(None);
            }
            names.entry(name).or_default().push(locant.into());
        }
    }
    if names.is_empty() {
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
