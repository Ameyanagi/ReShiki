//! #879 draws ²H/³H as D/T by default; the original writer drew 2H/3H.
//! ChemDraw exports of such drawings are compared after undoing exactly that:
//! CDXML gets back its superscript mass number and loses the `NumHydrogens`
//! that keeps ChemDraw from adding H to "D"; CDX is written with the 2H/3H
//! option, whose bytes match the original writer. Everything else is compared.
use reshiki::{document::Document, style::HydrogenIsotopes};

pub fn labelled(doc: &Document) -> bool {
    doc.atoms
        .iter()
        .any(|a| a.element == "H" && matches!(a.isotope, 2 | 3))
}

/// The document drawn with mass numbers instead of D/T.
pub fn mass_numbers(doc: &Document) -> Document {
    let mut doc = doc.clone();
    doc.drawing_style.hydrogen_isotopes = HydrogenIsotopes::MassNumbers;
    doc
}

/// The echoed document of a `mass_numbers` request, as requested. Any other
/// echoed mode is marked Letters so that it fails the comparison.
pub fn auto(doc: &mut Document) {
    doc.drawing_style.hydrogen_isotopes =
        if doc.drawing_style.hydrogen_isotopes == HydrogenIsotopes::MassNumbers {
            HydrogenIsotopes::Auto
        } else {
            HydrogenIsotopes::Letters
        };
}

/// Default CDXML with each D/T label written as the original 2H/3H label.
pub fn as_mass_numbers(xml: &str) -> String {
    let (mut out, mut rest, mut count) = (String::new(), xml, 0);
    while let Some(start) = rest.find("<n ") {
        let open = start + rest[start..].find('>').expect("Unterminated node");
        let tag = &rest[start..=open];
        let mass = ["2", "3"]
            .into_iter()
            .find(|m| tag.contains(&format!(" Element=\"1\" Isotope=\"{m}\" ")));
        let Some(mass) = mass.filter(|_| !tag.ends_with("/>")) else {
            out.push_str(&rest[..=open]);
            rest = &rest[open + 1..];
            continue;
        };
        let close = start + rest[start..].find("</n>").expect("Unclosed node") + 4;
        let node = &rest[start..close];
        assert_eq!(
            tag.matches(" NumHydrogens=\"0\"").count(),
            1,
            "D/T node: {tag}"
        );
        let letter = if mass == "2" { ">D</s>" } else { ">T</s>" };
        assert_eq!(node.matches(letter).count(), 1, "D/T label: {node}");
        let at = node.find(letter).expect("D/T label");
        let span = node[..at].rfind("<s ").expect("D/T span");
        let attributes = &node[span..at];
        assert_eq!(
            attributes.matches(" face=\"0\"").count(),
            1,
            "D/T face: {attributes}"
        );
        let superscript = attributes.replacen(" face=\"0\"", " face=\"64\"", 1);
        out.push_str(&rest[..start]);
        out.push_str(&node[..span].replacen(" NumHydrogens=\"0\"", "", 1));
        out.push_str(&format!("{superscript}>{mass}</s>{attributes}>H</s>"));
        out.push_str(&node[at + letter.len()..]);
        rest = &rest[close..];
        count += 1;
    }
    out.push_str(rest);
    assert!(count > 0, "No D/T label");
    out
}
