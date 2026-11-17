//! ChemDraw linked property text (`chemicalproperty`) in and out.
use super::*;
use crate::{
    document::{Document, Point},
    linked_properties::{Property, toggle},
    typography::TextFormat,
};

fn import(xml: &str) -> Document {
    import_cdxml(xml).unwrap().document
}
fn xml(d: &Document) -> String {
    let request = crate::engine::Request::molecule("export", d.clone());
    crate::exchange::drawing::write(d, (&request).into()).unwrap()
}
fn caption(d: &Document) -> &str {
    let id = d.property_captions[0].annotation;
    &d.annotations.iter().find(|a| a.id == id).unwrap().text
}

#[test]
fn chemdraw_property_text_imports_with_its_link() {
    let d = import(include_str!(
        "../../../../../tests/fixtures/chemdraw-properties/phenol-props.cdxml"
    ));
    assert_eq!(d.property_captions.len(), 1);
    assert_eq!(
        d.property_captions[0].properties,
        [
            Property::Formula,
            Property::Weight,
            Property::ExactMass,
            Property::Analysis
        ]
    );
    assert_eq!(d.property_captions[0].atoms.len(), 7);
    assert_eq!(
        caption(&d),
        "Chemical formula: C6H6O\nExact mass: 94.04\nMolecular weight: 94.11\nElemental analysis: C, 76.57; H, 6.43; O, 17.00"
    );
    let binary =
        include_bytes!("../../../../../tests/fixtures/chemdraw-properties/phenol-props.cdx");
    let from_cdx = import(&crate::exchange::from_cdx(binary).unwrap());
    assert_eq!(from_cdx.property_captions, d.property_captions);
    let edited = import(include_str!(
        "../../../../../tests/fixtures/chemdraw-properties/phenol-props-edited.cdxml"
    ));
    assert!(
        caption(&edited)
            .starts_with("Chemical formula: C7H8O\nExact mass: 108.06\nMolecular weight: 108.14")
    );
}

#[test]
fn chemdraw_text_whose_link_is_off_stays_plain() {
    let d = import(include_str!(
        "../../../../../tests/fixtures/chemdraw-properties/phenol-props-textedited.cdxml"
    ));
    assert!(d.property_captions.is_empty());
    assert!(
        d.annotations
            .iter()
            .any(|a| a.text.contains("Molecular weight: 108.14 x"))
    );
}

#[test]
fn chemdraw_text_over_a_salt_stays_plain_with_a_notice() {
    let salt = include_str!("../../../../../tests/fixtures/chemdraw-properties/salt-props.cdxml");
    let imported = import_cdxml(salt).unwrap();
    // ChemDraw's caption covers both ions. ReShiki links one molecule, so the
    // text keeps ChemDraw's values instead of changing to one ion's.
    assert!(imported.document.property_captions.is_empty());
    assert!(
        imported
            .document
            .annotations
            .iter()
            .any(|a| a.text.ends_with("Molecular weight: 116.09"))
    );
    assert_eq!(imported.notices.len(), 1, "{:?}", imported.notices);
    // A second caption over the phenoxide alone links to it.
    let d = import(include_str!(
        "../../../../../tests/fixtures/chemdraw-properties/salt-props-onefrag.cdxml"
    ));
    assert_eq!(d.property_captions.len(), 1);
    assert_eq!(d.property_captions[0].atoms.len(), 7);
    assert_eq!(d.annotations.len(), 2);
    assert_eq!(caption(&d), "Molecular weight: 93.11");
}

#[test]
fn chemdraw_captions_on_parts_of_one_molecule_keep_the_second_as_text() {
    // The file's own caption is switched off, so only the two below link.
    let source =
        include_str!("../../../../../tests/fixtures/chemdraw-properties/phenol-props.cdxml")
            .replace(
                r#"ChemicalPropertyIsActive="yes""#,
                r#"ChemicalPropertyIsActive="no""#,
            );
    let end = source.find("</page>").unwrap();
    let atoms: Vec<_> = source
        .split("<n\n id=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect();
    assert!(atoms.len() >= 2, "{source}");
    let xml = format!(
        r#"{}<t id="9001" p="0 0"><s size="10">Formula: C</s></t><chemicalproperty id="9002" ChemicalPropertyDisplayID="9001" ChemicalPropertyIsActive="yes" ChemicalPropertyType="2" BasisObjects="{}"/><t id="9003" p="0 40"><s size="10">Formula: O</s></t><chemicalproperty id="9004" ChemicalPropertyDisplayID="9003" ChemicalPropertyIsActive="yes" ChemicalPropertyType="2" BasisObjects="{}"/>{}"#,
        &source[..end],
        atoms[0],
        atoms[1],
        &source[end..]
    );
    let imported = import_cdxml(&xml).unwrap();
    let d = &imported.document;
    // ChemDraw scoped each to a selection. The first keeps its source text
    // and link until editing; the second remains ordinary text.
    assert_eq!(d.property_captions.len(), 1);
    assert_eq!(caption(d), "Formula: C");
    assert!(d.annotations.iter().any(|a| a.text == "Formula: O"));
    assert_eq!(imported.notices.len(), 1);
}

fn phenol() -> (Document, Vec<u64>) {
    let mut d = Document::default();
    let ring: Vec<_> = (0..6)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::PI / 3.;
            d.add_atom("C", Point::new(30. * angle.cos(), 30. * angle.sin()))
        })
        .collect();
    for i in 0..6 {
        d.add_bond(
            ring[i],
            ring[(i + 1) % 6],
            if i % 2 == 0 { 2 } else { 1 },
            "plain",
        );
    }
    let oxygen = d.add_atom("O", Point::new(60., 0.));
    d.add_bond(ring[0], oxygen, 1, "plain");
    let atoms = d.atoms.iter().map(|a| a.id).collect();
    (d, atoms)
}

#[test]
fn linked_captions_export_as_chemdraw_property_text_and_return() {
    let (mut d, atoms) = phenol();
    for p in [Property::Weight, Property::Formula] {
        toggle(&mut d, &atoms, p, TextFormat::default(), true).unwrap();
    }
    let x = xml(&d);
    let link = x
        .split("<chemicalproperty")
        .nth(1)
        .and_then(|rest| rest.split("/>").next())
        .unwrap();
    assert!(link.contains(r#"ChemicalPropertyIsActive="yes""#), "{link}");
    assert!(link.contains(r#"ChemicalPropertyType="2 4""#), "{link}");
    // The fragment, then its 7 atoms and 7 bonds.
    let basis = link.split(r#"BasisObjects=""#).nth(1).unwrap();
    let basis = basis.split('"').next().unwrap();
    assert_eq!(basis.split_whitespace().count(), 15, "{x}");
    let back = import(&x);
    assert_eq!(
        back.property_captions[0].properties,
        d.property_captions[0].properties
    );
    assert_eq!(caption(&back), caption(&d));
    let cdx = crate::exchange::to_cdx(&x).unwrap();
    // The live flag is a zero-length property in CDX.
    assert!(cdx.windows(4).any(|w| w == [0xb2, 0x0b, 0, 0]));
    let back = import(&crate::exchange::from_cdx(&cdx).unwrap());
    assert_eq!(back.property_captions.len(), 1);
    assert_eq!(caption(&back), caption(&d));
}

#[test]
fn the_name_line_exports_as_plain_text_above_linked_properties() {
    use crate::linked_properties::{named, rename};
    let (mut d, atoms) = phenol();
    for p in [Property::Weight, Property::Name] {
        toggle(&mut d, &atoms, p, TextFormat::default(), true).unwrap();
    }
    let smiles = d.property_captions[0].smiles.clone();
    named(&mut d, &smiles, "phenol");
    for custom in [false, true] {
        if custom {
            rename(&mut d, &atoms, "Compound 3a", TextFormat::default(), true).unwrap();
        }
        let name = if custom { "Compound 3a" } else { "phenol" };
        let x = xml(&d);
        // Only the property line is linked; ChemDraw keeps no linked name.
        assert!(x.contains(r#"ChemicalPropertyType="4""#), "{x}");
        for back in [
            import(&x),
            import(&crate::exchange::from_cdx(&crate::exchange::to_cdx(&x).unwrap()).unwrap()),
        ] {
            assert_eq!(back.property_captions[0].properties, [Property::Weight]);
            assert_eq!(caption(&back), "Mol. Wt.: 94.11");
            // The name comes back as bold text of its own, above the line.
            let text = back.annotations.iter().find(|a| a.text == name).unwrap();
            assert!(text.format.at(0).bold);
            let line = back
                .annotations
                .iter()
                .find(|a| a.id == back.property_captions[0].annotation)
                .unwrap();
            assert!(text.position.y < line.position.y);
            assert!(!line.format.at(0).bold);
        }
    }
    // Grouped with its molecule, the name stays in the group in ChemDraw.
    let ids = d.all_ids();
    d.group_selection(&ids).unwrap();
    let x = xml(&d);
    let group = &x[x.find("<group").unwrap()..x.rfind("</group>").unwrap()];
    assert!(group.contains("Compound 3a") && group.contains("Mol. Wt."), "{x}");
    // A name alone is plain text.
    toggle(
        &mut d,
        &atoms,
        Property::Weight,
        TextFormat::default(),
        true,
    )
    .unwrap();
    let x = xml(&d);
    assert!(!x.contains("chemicalproperty"), "{x}");
    assert_eq!(import(&x).annotations[0].text, "Compound 3a");
}

#[test]
fn captions_without_a_chemdraw_type_export_as_plain_text() {
    let (mut d, atoms) = phenol();
    toggle(&mut d, &atoms, Property::LogP, TextFormat::default(), true).unwrap();
    let x = xml(&d);
    assert!(!x.contains("chemicalproperty"));
    assert!(x.contains("cLogP: 1.39"));
    let back = import(&x);
    assert!(back.property_captions.is_empty());
    assert_eq!(back.annotations[0].text, "cLogP: 1.39");
}

#[test]
fn a_generic_structure_keeps_its_source_caption_on_passive_import() {
    let source = include_str!("../../../../../tests/fixtures/generic-structures/link-chain.cdxml");
    let end = source.find("</page>").unwrap();
    let fragment = source
        .split("<fragment")
        .nth(1)
        .and_then(|rest| rest.split("id=\"").nth(1))
        .and_then(|rest| rest.split('"').next())
        .unwrap();
    let xml = format!(
        r#"{}<t id="9001" p="0 0"><s size="10">Formula: C3H8</s></t><chemicalproperty id="9002" ChemicalPropertyDisplayID="9001" ChemicalPropertyIsActive="yes" ChemicalPropertyType="2" BasisObjects="{fragment}"/>{}"#,
        &source[..end],
        &source[end..]
    );
    let d = import(&xml);
    assert_eq!(d.property_captions.len(), 1);
    assert_eq!(caption(&d), "Formula: C3H8");
}
