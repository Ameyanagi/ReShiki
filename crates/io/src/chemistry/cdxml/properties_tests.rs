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
const PHENOL: &str = "Formula: C6H6O\nMol. Wt.: 94.11\nExact Mass: 94.0419\nAnal. Calcd for C6H6O: C, 76.57; H, 6.43";

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
    assert_eq!(caption(&d), PHENOL);
    let binary =
        include_bytes!("../../../../../tests/fixtures/chemdraw-properties/phenol-props.cdx");
    let from_cdx = import(&crate::exchange::from_cdx(binary).unwrap());
    assert_eq!(from_cdx.property_captions, d.property_captions);
    let edited = import(include_str!(
        "../../../../../tests/fixtures/chemdraw-properties/phenol-props-edited.cdxml"
    ));
    assert!(caption(&edited).starts_with("Formula: C7H8O\nMol. Wt.: 108.14"));
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
fn chemdraw_salt_properties_keep_one_link_per_atom() {
    let salt = include_str!("../../../../../tests/fixtures/chemdraw-properties/salt-props.cdxml");
    let d = import(salt);
    assert_eq!(d.property_captions.len(), 1);
    assert_eq!(d.property_captions[0].atoms.len(), 8);
    assert_eq!(caption(&d), "Formula: C6H5NaO\nMol. Wt.: 116.09");
    let binary = include_bytes!("../../../../../tests/fixtures/chemdraw-properties/salt-props.cdx");
    assert_eq!(
        import(&crate::exchange::from_cdx(binary).unwrap()).property_captions,
        d.property_captions
    );
    // A second link over the phenoxide alone stays as plain text.
    let d = import(include_str!(
        "../../../../../tests/fixtures/chemdraw-properties/salt-props-onefrag.cdxml"
    ));
    assert_eq!(d.property_captions.len(), 1);
    assert_eq!(d.annotations.len(), 2);
    assert!(
        d.annotations
            .iter()
            .any(|a| a.text == "Molecular weight: 93.11")
    );
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
