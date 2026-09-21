//! MOL bond stereo adapted from RDKit Chirality.cpp and FindStereo.cpp.
//! Copyright (C) 2004-2024 Greg Landrum and other RDKit contributors.
//! Copyright (C) 2020 Greg Landrum and T5 Informatics GmbH.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::*;

pub(crate) struct FileBond {
    pub a: usize,
    pub b: usize,
    pub code: u8,
}

pub(crate) fn file_bonds(
    input: &WedgeState,
    properties: &WedgeProperties,
    conformer: &Conformer,
    ranks: &[Option<u32>],
) -> Result<Vec<FileBond>, String> {
    if ranks.len() != input.graph.atoms.len() {
        return Err("MOL stereo rank count changed".into());
    }
    let mut work = Work(50_000_000);
    let mut ctx = Context::new(input, properties, conformer, true, &mut work)?;
    ctx.choose_tetrahedral(&mut work)?;
    ctx.choose_atrop(&mut work)?;
    ctx.state
        .graph
        .bonds
        .iter()
        .enumerate()
        .map(|(i, bond)| {
            work.spend(1)?;
            let mut reverse = false;
            let direction = if can_direct(bond.order) {
                let direction = match ctx.choices.get(&i) {
                    Some(Choice::Atom(atom)) => ctx.wedge_direction(i, *atom, &mut work)?,
                    Some(Choice::Atrop(direction)) => *direction,
                    None => ctx.direction(i)?,
                };
                if (wedged(direction) || direction == Direction::Unknown)
                    && let Some(Choice::Atom(atom)) = ctx.choices.get(&i)
                {
                    reverse = *atom != bond.a;
                }
                direction
            } else if bond.order == 2 && ctx.crossed(i, ranks, &mut work)? {
                Direction::EitherDouble
            } else {
                Direction::None
            };
            let (a, b) = if reverse {
                (bond.b, bond.a)
            } else {
                (bond.a, bond.b)
            };
            Ok(FileBond {
                a,
                b,
                code: match direction {
                    Direction::Wedge => 1,
                    Direction::Hash => 6,
                    Direction::Unknown => 4,
                    Direction::EitherDouble => 3,
                    _ => 0,
                },
            })
        })
        .collect()
}

impl Context<'_> {
    fn crossed(&self, id: usize, ranks: &[Option<u32>], work: &mut Work) -> Result<bool, String> {
        let bond = at(&self.state.graph.bonds, id)?;
        let stereo = at(&self.state.metadata.bonds, id)?.stereo;
        if stereo == 1 {
            for atom in [bond.a, bond.b] {
                for &other in at(&self.adjacent, atom)? {
                    work.spend(1)?;
                    if self.direction(other)? == Direction::Unknown
                        && at(&self.state.graph.bonds, other)?.a == atom
                    {
                        return Ok(false);
                    }
                }
            }
            return Ok(true);
        }
        if stereo != 0 || matches!(*at(&self.minimum, id)?, 1..=7) {
            return Ok(false);
        }
        for atom in [bond.a, bond.b] {
            let a = at(&self.state.graph.atoms, atom)?;
            let mut hydrogens = usize::from(a.explicit_hydrogens)
                + at(&self.properties.valences, atom)?.implicit_hydrogens as usize;
            for &edge in at(&self.adjacent, atom)? {
                work.spend(1)?;
                hydrogens += usize::from(
                    at(&self.state.graph.atoms, self.other(edge, atom)?)?.atomic_number == 1,
                );
            }
            if !matches!(self.total_degree(atom)?, 2 | 3) || hydrogens >= 2 {
                return Ok(false);
            }
        }
        if self.direction(id)? == Direction::EitherDouble {
            return Ok(true);
        }
        for atom in [bond.a, bond.b] {
            let degree = at(&self.adjacent, atom)?.len();
            let a = at(&self.state.graph.atoms, atom)?;
            let valence = at(&self.properties.valences, atom)?;
            if degree <= 1
                || i64::from(valence.explicit_valence)
                    - degree as i64
                    - i64::from(a.explicit_hydrogens)
                    != 1
            {
                return Ok(false);
            }
            let mut seen = HashSet::new();
            for &other in at(&self.adjacent, atom)? {
                work.spend(1)?;
                let nb = at(&self.state.graph.bonds, other)?;
                if nb.order != 1 {
                    continue;
                }
                let direction = self.direction(other)?;
                if matches!(direction, Direction::Up | Direction::Down)
                    || direction == Direction::Unknown && nb.a == atom
                {
                    return Ok(false);
                }
                if let Some(rank) = at(ranks, self.other(other, atom)?)?
                    && *rank <= i32::MAX as u32
                    && !seen.insert(*rank)
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
