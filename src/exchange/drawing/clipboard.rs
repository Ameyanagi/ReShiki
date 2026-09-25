//! Explicitly lossy presentation conversion for an external clipboard copy.
//! File export stays strict; the native clipboard keeps the complete original.
use super::*;

pub(crate) fn write(source: &Document) -> Result<(String, Vec<String>)> {
    source.validate().map_err(invalid)?;
    let haworth = crate::haworth::interchange::export_bonds(source).map_err(invalid)?;
    let mut document = source.clone();
    let mut charges = 0;
    let mut variables = 0;
    let mut curves = 0;
    let mut perspective = 0;
    for atom in &mut document.atoms {
        if atom.charge != 0 && atom.display.hide_charge && !super::ligands::hidden_charge(atom) {
            atom.display.hide_charge = false;
            charges += 1;
        }
        if atom.display.variable.is_some() {
            variables += 1;
        }
    }
    for (i, bond) in document.bonds.iter_mut().enumerate() {
        if bond.ring_arc {
            bond.ring_arc = false;
            curves += 1;
        }
        if bond.projection
            && bond.display != "plain"
            && !haworth.contains(&i)
            && !(bond.order == 4 && matches!(bond.display.as_str(), "bold" | "wedge"))
        {
            if [bond.a, bond.b]
                .iter()
                .any(|id| source.atom(*id).is_some_and(|a| a.stereo.is_some()))
            {
                return Err(invalid(
                    "Cannot simplify a projected bond at an assigned stereocenter",
                ));
            }
            bond.display = "plain".into();
            perspective += 1;
        }
    }
    let mut notices = Vec::new();
    if charges != 0 {
        notices.push(format!("Editable CDX copy shows {charges} hidden charge label(s); chemical charges are unchanged"));
    }
    if variables != 0 {
        notices.push(format!("Editable CDX copy keeps {variables} variable label(s) as uninterpreted atom text, without query semantics"));
    }
    if curves != 0 {
        notices.push(
            "Editable CDX copy omits partial inner-ring curves; bond orders are unchanged".into(),
        );
    }
    if perspective != 0 {
        notices.push(format!("Editable CDX copy uses plain lines for {perspective} perspective-emphasis bond(s); ordinary stereochemical wedges are unchanged"));
    }
    let request = Request::molecule("export", document);
    let snapshot = request
        .document
        .as_ref()
        .ok_or_else(|| invalid("Missing clipboard snapshot"))?;
    let xml = write_impl(snapshot, (&request).into(), true, true)?;
    Ok((xml, notices))
}
