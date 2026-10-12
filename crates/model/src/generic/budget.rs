//! Shared expansion work allowance and drawing-size accounting.
use crate::document::Document;

pub(super) struct Work {
    pub(super) remaining: usize,
}
impl Work {
    pub(super) fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }
    pub(super) fn charge(&mut self, amount: usize) -> Result<(), String> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or("Generic expansion exceeds the work limit")?;
        Ok(())
    }
}
pub(super) fn drawing_size(doc: &Document) -> usize {
    doc.atoms
        .len()
        .saturating_add(doc.bonds.len())
        .saturating_add(doc.annotations.len())
        .saturating_add(doc.arrows.len())
        .saturating_add(doc.graphics.len())
        .saturating_add(
            doc.groups
                .iter()
                .map(|g| g.members.len() + 1)
                .sum::<usize>(),
        )
        .saturating_add(
            doc.abbreviations
                .iter()
                .map(|g| g.members.len())
                .sum::<usize>(),
        )
}
