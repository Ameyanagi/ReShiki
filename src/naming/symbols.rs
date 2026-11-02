//! Expand only complete, recognised biochemical symbol notation before OPSIN.
use super::Error;

mod glycan;
mod peptide;

pub(super) struct Expansion {
    pub name: String,
    pub warnings: Vec<String>,
}

pub(super) fn expand(name: &str) -> Result<Option<Expansion>, Error> {
    let expansion = match peptide::expand(name)? {
        Some(expansion) => Some(expansion),
        None => glycan::expand(name)?,
    };
    if expansion
        .as_ref()
        .is_some_and(|expanded| expanded.name.len() > super::worker::MAX_NAME_BYTES)
    {
        return Err(Error::Rejected(
            "The expanded name exceeds the local parser limit of 2048 bytes".into(),
        ));
    }
    Ok(expansion)
}

pub(super) fn peptide_sequence(name: &str) -> Result<String, Error> {
    peptide::one_letter(name)
}
