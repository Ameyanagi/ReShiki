use super::*;
use crate::{document::Point, editing};

#[test]
fn changed_chemical_identity_cannot_publish_a_display_edit() -> anyhow::Result<()> {
    let mut original = Document::default();
    editing::ring(&mut original, Point::default(), 6, false, 42.);
    for (i, bond) in original.bonds.iter_mut().enumerate() {
        bond.order = if i % 2 == 0 { 2 } else { 1 };
    }
    let selected = original.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
    for variant in 0..3 {
        let mut draft = aromatic_display(&original, &selected)?;
        match variant {
            0 => {
                draft
                    .after
                    .state
                    .graph
                    .atoms
                    .first_mut()
                    .ok_or_else(|| anyhow::anyhow!("Missing atom"))?
                    .isotope = 13
            }
            1 => {
                draft
                    .after
                    .state
                    .metadata
                    .atoms
                    .first_mut()
                    .ok_or_else(|| anyhow::anyhow!("Missing atom metadata"))?
                    .map_number = 7
            }
            _ => {
                draft
                    .after
                    .state
                    .graph
                    .bonds
                    .first_mut()
                    .ok_or_else(|| anyhow::anyhow!("Missing bond"))?
                    .b = usize::MAX
            }
        }
        assert!(
            draft.finish().is_err(),
            "Changed identity variant {variant}"
        );
    }
    Ok(())
}
