//! Matching order follows RDKit's unsorted VF2 query traversal: lowest query
//! vertex, then the original target adjacency order, with at most 1,000 maps.
//! Expression semantics follow RDKit QueryOps.h. No runtime SMARTS parser.
//! Query predicates: Copyright (C) 2003-2021 Greg Landrum and contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE.
use super::{Target, Work, at};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pattern {
    atoms: Vec<Expr>,
    bonds: Vec<Edge>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edge {
    a: usize,
    b: usize,
    query: Expr,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expr {
    op: Op,
    #[serde(default)]
    value: i32,
    #[serde(default)]
    children: Vec<Expr>,
}
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Op {
    All,
    Any,
    Not,
    Always,
    AtomType,
    Number,
    Hydrogens,
    Degree,
    Valence,
    Charge,
    Aliphatic,
    Aromatic,
    BondOrder,
    SingleOrAromatic,
    RingBond,
    Recursive,
}

fn check_expr(expr: &Expr, pattern: usize, depth: usize) -> Result<(), String> {
    if depth > 24 || expr.children.len() > 16 {
        return Err("Descriptor expression limit exceeded".into());
    }
    match expr.op {
        Op::All | Op::Any if expr.children.is_empty() => {
            return Err("Empty descriptor expression".into());
        }
        Op::Not if expr.children.len() != 1 => return Err("Invalid descriptor negation".into()),
        Op::Recursive if expr.value < 0 || expr.value as usize >= pattern => {
            return Err("Cyclic descriptor subquery".into());
        }
        _ => {}
    }
    for child in &expr.children {
        check_expr(child, pattern, depth + 1)?;
    }
    Ok(())
}
pub(super) fn validate(patterns: &[Pattern]) -> Result<(), String> {
    if patterns.len() > 256 {
        return Err("Descriptor pattern limit exceeded".into());
    }
    for (id, p) in patterns.iter().enumerate() {
        if p.atoms.is_empty() || p.atoms.len() > 16 || p.bonds.len() > 32 {
            return Err("Descriptor query size limit exceeded".into());
        }
        for expr in &p.atoms {
            check_expr(expr, id, 0)?;
        }
        for b in &p.bonds {
            if b.a == b.b || b.a >= p.atoms.len() || b.b >= p.atoms.len() {
                return Err("Invalid descriptor query edge".into());
            }
            check_expr(&b.query, id, 0)?;
        }
        // All pinned patterns introduce each new atom through an earlier one.
        for i in 1..p.atoms.len() {
            if !p
                .bonds
                .iter()
                .any(|b| (b.a == i && b.b < i) || (b.b == i && b.a < i))
            {
                return Err("Disconnected descriptor pattern".into());
            }
        }
    }
    Ok(())
}

enum Candidates {
    All(usize),
    Neighbors(usize, usize),
}
impl Candidates {
    fn next(&mut self, target: &Target) -> Result<Option<usize>, String> {
        match self {
            Self::All(i) => {
                if *i >= target.nodes.len() {
                    return Ok(None);
                }
                let id = *i;
                *i += 1;
                Ok(Some(id))
            }
            Self::Neighbors(atom, i) => {
                let result = at(&target.adjacent, *atom)?.get(*i).map(|&(a, _)| a);
                if result.is_some() {
                    *i += 1;
                }
                Ok(result)
            }
        }
    }
}
pub(super) struct Matcher<'a> {
    target: &'a Target,
    patterns: &'a [Pattern],
    recursive: HashMap<usize, HashSet<usize>>,
    work: &'a mut Work,
}
impl<'a> Matcher<'a> {
    pub(super) fn new(target: &'a Target, patterns: &'a [Pattern], work: &'a mut Work) -> Self {
        Self {
            target,
            patterns,
            recursive: HashMap::new(),
            work,
        }
    }
    fn subqueries(expr: &Expr, ids: &mut Vec<usize>) {
        if matches!(expr.op, Op::Recursive) {
            ids.push(expr.value as usize);
        }
        for child in &expr.children {
            Self::subqueries(child, ids);
        }
    }
    fn atom(&mut self, expr: &Expr, id: usize) -> Result<bool, String> {
        self.work.spend(1)?;
        let node = at(&self.target.nodes, id)?;
        Ok(match expr.op {
            Op::All => {
                for child in &expr.children {
                    if !self.atom(child, id)? {
                        return Ok(false);
                    }
                }
                true
            }
            Op::Any => {
                for child in &expr.children {
                    if self.atom(child, id)? {
                        return Ok(true);
                    }
                }
                false
            }
            Op::Not => !self.atom(at(&expr.children, 0)?, id)?,
            Op::Always => true,
            Op::AtomType => i32::from(node.number) + 1000 * i32::from(node.aromatic) == expr.value,
            Op::Number => i32::from(node.number) == expr.value,
            Op::Hydrogens => i64::from(node.hydrogens) == i64::from(expr.value),
            Op::Degree => node.degree as i64 == i64::from(expr.value),
            Op::Valence => i64::from(node.valence) == i64::from(expr.value),
            Op::Charge => i32::from(node.charge) == expr.value,
            Op::Aliphatic => i32::from(!node.aromatic) == expr.value,
            Op::Aromatic => i32::from(node.aromatic) == expr.value,
            Op::Recursive => self
                .recursive
                .get(&(expr.value as usize))
                .ok_or("Missing recursive descriptor matches")?
                .contains(&id),
            _ => return Err("Bond predicate used on an atom".into()),
        })
    }
    fn bond(&mut self, expr: &Expr, id: usize) -> Result<bool, String> {
        self.work.spend(1)?;
        let bond = at(&self.target.edges, id)?;
        Ok(match expr.op {
            Op::All => {
                for child in &expr.children {
                    if !self.bond(child, id)? {
                        return Ok(false);
                    }
                }
                true
            }
            Op::Any => {
                for child in &expr.children {
                    if self.bond(child, id)? {
                        return Ok(true);
                    }
                }
                false
            }
            Op::Not => !self.bond(at(&expr.children, 0)?, id)?,
            Op::Always => true,
            Op::BondOrder => match expr.value {
                1..=3 => i32::from(bond.order) == expr.value,
                12 => bond.order == 4,
                _ => return Err("Unsupported descriptor bond query".into()),
            },
            Op::SingleOrAromatic => matches!(bond.order, 1 | 4),
            Op::RingBond => i32::from(bond.in_ring) == expr.value,
            _ => return Err("Atom predicate used on a bond".into()),
        })
    }
    pub(super) fn roots(&mut self, id: usize) -> Result<Vec<usize>, String> {
        let pattern = at(self.patterns, id)?;
        let mut dependencies = Vec::new();
        for expr in &pattern.atoms {
            Self::subqueries(expr, &mut dependencies);
        }
        for child in dependencies {
            if !self.recursive.contains_key(&child) {
                let roots = self.roots(child)?;
                self.recursive.insert(child, roots.into_iter().collect());
            }
        }
        let mut result = Vec::new();
        let mut mapping = vec![None; pattern.atoms.len()];
        let mut frames = vec![Candidates::All(0)];
        while !frames.is_empty() {
            self.work.spend(1)?;
            let depth = frames.len() - 1;
            *mapping.get_mut(depth).ok_or("Missing query mapping")? = None;
            let Some(candidate) = frames
                .last_mut()
                .ok_or("Missing query frame")?
                .next(self.target)?
            else {
                frames.pop();
                continue;
            };
            if mapping.contains(&Some(candidate))
                || !self.atom(at(&pattern.atoms, depth)?, candidate)?
            {
                continue;
            }
            let mut compatible = true;
            for edge in &pattern.bonds {
                let other = if edge.a == depth {
                    edge.b
                } else if edge.b == depth {
                    edge.a
                } else {
                    continue;
                };
                let Some(mapped) = *at(&mapping, other)? else {
                    continue;
                };
                let adjacent = at(&self.target.adjacent, candidate)?;
                self.work.spend(adjacent.len())?;
                let Some(&(_, bond)) = adjacent.iter().find(|&&(a, _)| a == mapped) else {
                    compatible = false;
                    break;
                };
                if !self.bond(&edge.query, bond)? {
                    compatible = false;
                    break;
                }
            }
            if !compatible {
                continue;
            }
            *mapping.get_mut(depth).ok_or("Missing query mapping")? = Some(candidate);
            if depth + 1 == pattern.atoms.len() {
                result.push(at(&mapping, 0)?.ok_or("Missing query root")?);
                if result.len() == 1000 {
                    break;
                }
            } else {
                let next = depth + 1;
                let parent = pattern
                    .bonds
                    .iter()
                    .find_map(|b| {
                        let other = if b.a == next {
                            b.b
                        } else if b.b == next {
                            b.a
                        } else {
                            return None;
                        };
                        mapping.get(other).copied().flatten()
                    })
                    .ok_or("Disconnected query traversal")?;
                frames.push(Candidates::Neighbors(parent, 0));
            }
        }
        Ok(result)
    }
}
