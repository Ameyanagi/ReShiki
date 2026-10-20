//! In-place donor-to-metal contacts preserve both endpoint atoms.
use crate::document::{Atom, Document};

/// Whether an explicit donor-to-metal contact changed the drawing.
#[derive(Debug, Clone, PartialEq)]
// Keep the owned document inline so returning a contact adds no boxing allocation.
#[allow(clippy::large_enum_variant)]
pub enum Coordination {
    /// A new directed dative contact was added to this document.
    Added(Document),
    /// The requested directed dative contact already exists.
    AlreadyPresent,
}

impl Coordination {
    /// Return the drawing to install, including an unchanged copy for an existing contact.
    pub fn into_document(self, source: &Document) -> Document {
        match self {
            Self::Added(document) => document,
            Self::AlreadyPresent => source.clone(),
        }
    }
}

/// Connect an existing lone-pair donor to a transition metal without changing
/// either atom's identity, position, charge, hydrogen metadata or covalent bonds.
/// This explicit drawing operation does not infer metal geometry or complex stereo.
pub fn coordinate_atoms(source: &Document, donor: u64, metal: u64) -> Result<Coordination, String> {
    source.validate()?;
    if donor == metal {
        return Err("Choose a donor and a different metal atom.".into());
    }
    let d = source.atom(donor).ok_or("The donor atom is unavailable.")?;
    let m = source.atom(metal).ok_or("The metal atom is unavailable.")?;
    if crate::generic::touches(source, &[donor, metal]) {
        return Err(crate::generic::JOIN_NOTICE.into());
    }
    super::shown_in_3d(source, &[donor, metal])?;
    for a in [d, m] {
        if !a.centroid.is_empty() || a.attachment.is_some() {
            const NOTICE: &str = "Coordination in place requires single chemical atoms; centroids and multi-center/variable attachment points are unsupported here.";
            return Err(NOTICE.into());
        }
        if source
            .abbreviations
            .iter()
            .any(|g| g.members.contains(&a.id))
        {
            return Err("Expand the abbreviation before using its atoms for coordination.".into());
        }
        if a.stereo.is_some() || a.radical_electrons != 0 {
            return Err(
                "Coordination endpoints with atom stereochemistry or radicals are unsupported."
                    .into(),
            );
        }
    }
    // These checks bound the supported lone-pair donor domain without changing
    // the existing capacity table used for ordinary covalent attachment.
    let limit = match (d.element.as_str(), d.charge) {
        ("N" | "P", 0) => 6,
        ("N" | "P", -1) | ("O" | "S", 0) => 4,
        ("O" | "S", -1) => 2,
        _ => {
            return Err(
                "Choose a neutral or −1 charged N, O, S or P donor. This endpoint is unsupported."
                    .into(),
            );
        }
    };
    if crate::templates::valence(source, donor).saturating_add(d.explicit_h.saturating_mul(2))
        > limit
    {
        return Err("The donor has no supported lone pair at its current covalent valence.".into());
    }
    if !coordination_metal(m) {
        return Err("Choose a supported transition-metal acceptor (Sc–Zn, Y–Cd or Hf–Hg).".into());
    }
    if let Some(bond) = source
        .bonds
        .iter()
        .find(|b| (b.a == donor && b.b == metal) || (b.a == metal && b.b == donor))
    {
        return if bond.is_dative() && bond.a == donor {
            Ok(Coordination::AlreadyPresent)
        } else {
            const NOTICE: &str = "These atoms already have a different bond; remove it explicitly before coordinating.";
            Err(NOTICE.into())
        };
    }
    if source
        .bonds
        .iter()
        .filter(|b| b.a == metal || b.b == metal)
        .count()
        >= 12
    {
        return Err("Coordination drawing supports at most 12 contacts at a metal atom.".into());
    }
    if d.position.distance(m.position) < 0.001 {
        return Err("The donor and metal overlap; separate them before connecting.".into());
    }
    let mut candidate = source.clone();
    // Existing directed order 5 consumes no donor covalent valence. Do not use
    // invalidate_chemistry here: donor H/stereo and remote chemistry must survive.
    crate::atom_labels::clear_computed(&mut candidate);
    candidate.bonds.push(crate::document::Bond::plain(
        donor,
        metal,
        crate::bonds::DATIVE,
        "plain",
    ));
    candidate.reconcile_molecule_groups();
    crate::reactions::reconcile(&mut candidate)?;
    candidate.validate()?;
    Ok(Coordination::Added(candidate))
}

/// Donor elements offered by the explicit coordination drawing workflow.
pub fn coordination_donor(atom: &Atom) -> bool {
    matches!(atom.element.as_str(), "N" | "O" | "S" | "P")
}

/// Supported acceptor elements for the explicit coordination drawing workflow.
pub fn coordination_metal(atom: &Atom) -> bool {
    matches!(
        atom.element.as_str(),
        "Sc" | "Ti"
            | "V"
            | "Cr"
            | "Mn"
            | "Fe"
            | "Co"
            | "Ni"
            | "Cu"
            | "Zn"
            | "Y"
            | "Zr"
            | "Nb"
            | "Mo"
            | "Tc"
            | "Ru"
            | "Rh"
            | "Pd"
            | "Ag"
            | "Cd"
            | "Hf"
            | "Ta"
            | "W"
            | "Re"
            | "Os"
            | "Ir"
            | "Pt"
            | "Au"
            | "Hg"
    )
}

#[cfg(test)]
mod tests;
