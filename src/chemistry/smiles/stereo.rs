//! Traversal-dependent stereochemistry from RDKit Canon.cpp.
//! Copyright (C) 2001-2021 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
mod atoms;
mod bonds;
use super::traversal::{Token, Traversal};
use crate::chemistry::{
    kekulize::Direction,
    ranking::Metadata,
    stereo::perception::{self, State},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid SMILES stereo state: {0}")]
    Invalid(String),
    #[error("SMILES stereo work limit exceeded")]
    Limit,
    #[error(transparent)]
    Permutation(#[from] super::Error),
}
pub struct Output {
    pub metadata: Metadata,
    pub directions: Vec<Direction>,
}
/// Adjust a complete connected component after perception and traversal. The
/// standard writer removes enhanced groups before ranking. Input is unchanged.
pub fn canonicalize(
    input: &State,
    walk: &Traversal,
    broken: &[bool],
    isomeric: bool,
) -> Result<Output, Error> {
    let mut ctx = Context::new(input, walk, broken)?;
    if isomeric {
        ctx.atoms()?;
    }
    ctx.bonds()?;
    Ok(ctx.output)
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn at<T>(values: &[T], id: usize) -> Result<&T, Error> {
    values.get(id).ok_or_else(|| invalid("Missing stereo item"))
}
fn put<T>(values: &mut [T], id: usize, value: T) -> Result<(), Error> {
    *values
        .get_mut(id)
        .ok_or_else(|| invalid("Missing stereo item"))? = value;
    Ok(())
}
struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), Error> {
        self.0 = self.0.checked_sub(amount).ok_or(Error::Limit)?;
        Ok(())
    }
}
struct Context<'a> {
    input: &'a State,
    walk: &'a Traversal,
    broken: &'a [bool],
    adjacent: Vec<Vec<usize>>,
    atom_visit: Vec<usize>,
    bond_visit: Vec<usize>,
    closures: Vec<bool>,
    atom_counts: Vec<i32>,
    bond_counts: Vec<i32>,
    output: Output,
    work: Work,
}
impl<'a> Context<'a> {
    fn new(input: &'a State, walk: &'a Traversal, broken: &'a [bool]) -> Result<Self, Error> {
        input
            .graph
            .cached_valences(Some(&input.valences))
            .map_err(invalid)?;
        input.metadata.validate(&input.graph).map_err(invalid)?;
        let (n, e) = (input.graph.atoms.len(), input.graph.bonds.len());
        if input.directions.len() != e
            || input.hybridizations.len() != n
            || input.conjugated.len() != e
            || input.properties.atoms.len() != n
            || broken.len() != n
            || walk.atom_bond_order().len() != n
            || walk.ring_closures().len() != n
            || input.properties.done.is_none()
            || !input.metadata.groups.is_empty()
        {
            return Err(invalid(
                "Component must have perceived stereo and matching annotations",
            ));
        }
        let mut stored = 0usize;
        for props in &input.properties.atoms {
            if let Some(members) = &props.ring_members {
                stored = stored.checked_add(members.len()).ok_or(Error::Limit)?;
                if stored > 2_000_000
                    || members
                        .iter()
                        .any(|&v| v == 0 || u64::from(v.unsigned_abs()) > n as u64)
                {
                    return Err(invalid("Invalid ring stereo members"));
                }
            }
        }
        let mut adjacent = vec![Vec::new(); n];
        for (id, b) in input.graph.bonds.iter().enumerate() {
            for a in [b.a, b.b] {
                adjacent.get_mut(a).ok_or(Error::Limit)?.push(id);
            }
        }
        let mut atom_visit = vec![0; n];
        let mut bond_visit = vec![0; e];
        for (position, token) in walk.tokens().iter().enumerate() {
            match *token {
                Token::Atom(a) => {
                    if *at(&atom_visit, a)? != 0 {
                        return Err(invalid("Repeated atom visit"));
                    }
                    put(&mut atom_visit, a, position + 1)?;
                }
                Token::Bond { index, left } => {
                    let bond = at(&input.graph.bonds, index)?;
                    if *at(&bond_visit, index)? != 0 || ![bond.a, bond.b].contains(&left) {
                        return Err(invalid("Traversal bond changed"));
                    }
                    put(&mut bond_visit, index, position + 1)?;
                }
                _ => (),
            }
        }
        if n == 0 || atom_visit.contains(&0) || bond_visit.contains(&0) {
            return Err(invalid("Incomplete component traversal"));
        }
        let mut closures = vec![false; e];
        for (atom, expected) in adjacent.iter().enumerate() {
            let mut order = at(walk.atom_bond_order(), atom)?.clone();
            order.sort_unstable();
            if order != *expected {
                return Err(invalid("Traversal neighbors changed"));
            }
            for &b in at(walk.ring_closures(), atom)? {
                put(&mut closures, b, true)?;
            }
        }
        let directions = input
            .directions
            .iter()
            .map(|&d| {
                if matches!(d, Direction::Up | Direction::Down) {
                    Direction::None
                } else {
                    d
                }
            })
            .collect();
        Ok(Self {
            input,
            walk,
            broken,
            adjacent,
            atom_visit,
            bond_visit,
            closures,
            atom_counts: vec![0; n],
            bond_counts: vec![0; e],
            work: Work(50_000_000),
            output: Output {
                metadata: input.metadata.clone(),
                directions,
            },
        })
    }
    fn other(&self, bond: usize, atom: usize) -> Result<usize, Error> {
        let b = at(&self.input.graph.bonds, bond)?;
        if b.a == atom {
            Ok(b.b)
        } else if b.b == atom {
            Ok(b.a)
        } else {
            Err(invalid("Stereo endpoint changed"))
        }
    }
    fn increment(&mut self, bond: usize, atom: usize) -> Result<(), Error> {
        count(&mut self.bond_counts, bond, 1)?;
        count(&mut self.atom_counts, atom, 1)
    }
}
fn count(values: &mut [i32], id: usize, delta: i32) -> Result<(), Error> {
    let value = values.get_mut(id).ok_or(Error::Limit)?;
    *value = value.checked_add(delta).ok_or(Error::Limit)?;
    Ok(())
}
