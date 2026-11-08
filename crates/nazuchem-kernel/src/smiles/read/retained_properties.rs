// Copied from crates/chemistry/src/smiles/read/retained_properties.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3; changed: pass an explicit default import budget
#[test]
fn hydrogen_removal_retains_raw_properties_by_original_atom() -> anyhow::Result<()> {
    let imported = super::read(
        "[H]CO |atomProp:0._CIPRank.dropped:1._chiralAtomRank.bad:2._CIPRank.4294967295|",
        &crate::budget::Budget::default(),
    )?;
    assert_eq!(imported.prepared.state.graph.atoms.len(), 2);
    assert_eq!(
        imported.reaction_properties,
        vec![
            vec![(b"_chiralAtomRank".to_vec(), b"bad".to_vec())],
            vec![(b"_CIPRank".to_vec(), b"4294967295".to_vec())],
        ]
    );
    Ok(())
}
