//! In-place donor-to-metal contacts preserve both endpoint atoms.
use crate::document::{Atom, Document};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    TooFar,
    Rejected(String),
}

/// Directed (donor, metal) pairs and contacts excluded by the reach rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinationPlan {
    pub pairs: Vec<(u64, u64)>,
    pub skipped: Vec<(u64, u64, Skip)>,
}

/// Plan an explicit metal/non-metal pair, or nearby donors in a larger selection.
pub fn coordination_plan(doc: &Document, selected: &[u64]) -> Option<CoordinationPlan> {
    let metals: Vec<_> = selected
        .iter()
        .filter_map(|id| doc.atom(*id))
        .filter(|atom| coordination_metal(atom))
        .collect();
    if let [first, second] = selected {
        let [metal] = metals.as_slice() else {
            return None;
        };
        let donor = if metal.id == *first { *second } else { *first };
        return Some(CoordinationPlan {
            pairs: vec![(donor, metal.id)],
            skipped: vec![],
        });
    }
    if selected.len() < 3 || metals.is_empty() {
        return None;
    }
    let mut plan = CoordinationPlan {
        pairs: vec![],
        skipped: vec![],
    };
    for donor in selected
        .iter()
        .filter_map(|id| doc.atom(*id))
        .filter(|atom| coordination_donor(atom) && !coordination_metal(atom))
    {
        let (metal, distance) = metals
            .iter()
            .map(|metal| (*metal, donor.position.distance(metal.position)))
            .min_by(|(a, da), (b, db)| da.total_cmp(db).then_with(|| a.id.cmp(&b.id)))?;
        if distance <= 2. * doc.drawing_style.bond_length_world {
            plan.pairs.push((donor.id, metal.id));
        } else {
            plan.skipped.push((donor.id, metal.id, Skip::TooFar));
        }
    }
    plan.pairs
        .sort_unstable_by_key(|&(donor, metal)| (metal, donor));
    plan.skipped
        .sort_unstable_by_key(|&(donor, metal, _)| (metal, donor));
    Some(plan)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManyOutcome {
    pub document: Document,
    pub added: Vec<(u64, u64)>,
    pub present: Vec<(u64, u64)>,
    pub skipped: Vec<(u64, u64, Skip)>,
}

/// A failed batch retains donor and metal IDs for caller-provided status text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManyError {
    pub message: String,
    pub skipped: Vec<(u64, u64, Skip)>,
}

/// Apply contacts to one working document, retaining each pair's result.
pub fn coordinate_many(doc: &Document, plan: &CoordinationPlan) -> Result<ManyOutcome, ManyError> {
    let mut outcome = ManyOutcome {
        document: doc.clone(),
        added: vec![],
        present: vec![],
        skipped: plan.skipped.clone(),
    };
    for &(donor, metal) in &plan.pairs {
        match coordinate_atoms(&outcome.document, donor, metal) {
            Ok(Coordination::Added(document)) => {
                outcome.document = document;
                outcome.added.push((donor, metal));
            }
            Ok(Coordination::AlreadyPresent) => outcome.present.push((donor, metal)),
            Err(message) => outcome
                .skipped
                .push((donor, metal, Skip::Rejected(message))),
        }
    }
    if !outcome.added.is_empty() || (!outcome.present.is_empty() && outcome.skipped.is_empty()) {
        return Ok(outcome);
    }
    let message = match outcome.skipped.first() {
        Some((_, _, Skip::TooFar)) => "Donor is too far from the metal.".into(),
        Some((_, _, Skip::Rejected(message))) => message.clone(),
        None => "No coordination donors.".into(),
    };
    Err(ManyError {
        message,
        skipped: outcome.skipped,
    })
}

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
    coordination_metal_element(&atom.element)
}

/// Supported metal symbols for coordination drawing and depiction exchange.
pub fn coordination_metal_element(element: &str) -> bool {
    matches!(
        element,
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
mod planning_tests;
#[cfg(test)]
mod tests;
