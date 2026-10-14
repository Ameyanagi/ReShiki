use super::*;
#[test]
fn indexed_metadata_cannot_bleed_between_spectra() {
    assert_eq!(
        numbered(
            "0:Chloroform-D1 (CDCl3) 10:Unreported 1:Benzene-D6 (C6D6)",
            "1"
        )
        .as_deref(),
        Some("Benzene-D6 (C6D6)")
    );
    assert_eq!(
        numbered("1: 2:Quantum ESPRESSO 3:", "1"),
        Some(String::new())
    );
    assert!(numbered("2:ACD/Labs 2020.1.0", "0").is_none());
}
#[test]
fn duplicate_spectra_and_equivalent_atoms_do_not_inflate_support() {
    let codes = vec![(2, "a".into())];
    let o = |id: &str, ppm| Observation {
        molecule_id: id.into(),
        connectivity: id.into(),
        nucleus: Nucleus::H1,
        parent: 0,
        ppm,
        codes: codes.clone(),
    };
    let index = build(&[o("a", 1.), o("a", 1.), o("a", 3.), o("b", 5.)], false).unwrap();
    let s = index.entries.values().next().unwrap();
    assert_eq!(s.support, 2);
    assert_eq!(s.median, 3.);
}
#[test]
fn retained_source_fixture_separates_calculation_metadata_from_observations() {
    let fixture = include_str!("../../../../../tests/fixtures/nmr/mixed-measured-calculated.sd");
    let metadata = fields(fixture);
    assert!(numbered(metadata.get("Program").unwrap(), "1").is_some());
    assert!(numbered(metadata.get("Program").unwrap(), "2").is_some());
    assert!(numbered(metadata.get("Program").unwrap(), "4").is_none());
    assert_eq!(
        numbered(metadata.get("Solvent").unwrap(), "4").as_deref(),
        Some("Chloroform-D1 (CDCl3)")
    );
    assert_eq!(
        numbered(metadata.get("Temperature [K]").unwrap(), "4").as_deref(),
        Some("298")
    );
    assert!(metadata.contains_key("Spectrum 1H 4"));
}
