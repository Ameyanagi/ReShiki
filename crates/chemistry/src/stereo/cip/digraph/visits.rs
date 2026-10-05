//! Persistent visit maps share unchanged branches instead of copying an entire
//! molecule-sized byte vector for every CIP node. Indices never escape the arena.
use super::{Error, Work, at, invalid};
use std::{num::NonZeroU32, os::raw::c_char};

type Link = Option<NonZeroU32>;
#[derive(Clone, Copy, Default)]
struct Cell {
    children: [Link; 2],
    value: c_char,
}
pub(super) struct Visits {
    cells: Vec<Cell>,
    atoms: usize,
    levels: u32,
}
impl Visits {
    pub(super) fn storage_bytes(&self) -> usize {
        self.cells
            .capacity()
            .saturating_mul(std::mem::size_of::<Cell>())
    }
    #[cfg(test)]
    pub(super) fn cells_len(&self) -> usize {
        self.cells.len()
    }
    pub(super) fn new(atoms: usize) -> Self {
        Self {
            cells: Vec::new(),
            atoms,
            levels: usize::BITS - atoms.saturating_sub(1).leading_zeros(),
        }
    }
    fn cell(&self, link: Link) -> Result<Cell, Error> {
        link.map(|i| at(&self.cells, (i.get() - 1) as usize).copied())
            .transpose()
            .map(|cell| cell.unwrap_or_default())
    }
    fn add(&mut self, cell: Cell) -> Result<Link, Error> {
        if self.cells.len() >= 2_000_000 {
            return Err(Error::Limit);
        }
        self.cells.push(cell);
        Ok(NonZeroU32::new(
            u32::try_from(self.cells.len()).map_err(|_| Error::Limit)?,
        ))
    }
    pub(super) fn get(
        &self,
        mut root: Link,
        atom: usize,
        work: &mut Work,
    ) -> Result<c_char, Error> {
        if atom >= self.atoms {
            return Err(invalid("Missing visit atom"));
        }
        work.spend(self.levels as usize + 1)?;
        for bit in (0..self.levels).rev() {
            root = *at(&self.cell(root)?.children, (atom >> bit) & 1)?;
        }
        Ok(self.cell(root)?.value)
    }
    pub(super) fn set(
        &mut self,
        mut root: Link,
        atom: usize,
        value: c_char,
        work: &mut Work,
    ) -> Result<Link, Error> {
        if atom >= self.atoms {
            return Err(invalid("Missing visit atom"));
        }
        work.spend(self.levels as usize + 1)?;
        let mut path = Vec::with_capacity(self.levels as usize);
        for bit in (0..self.levels).rev() {
            let cell = self.cell(root)?;
            let side = (atom >> bit) & 1;
            root = *at(&cell.children, side)?;
            path.push((cell, side));
        }
        let mut child = self.add(Cell {
            value,
            ..Cell::default()
        })?;
        for (mut cell, side) in path.into_iter().rev() {
            *cell
                .children
                .get_mut(side)
                .ok_or_else(|| invalid("Invalid visit branch"))? = child;
            child = self.add(cell)?;
        }
        Ok(child)
    }
}
