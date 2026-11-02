use super::{hydrogens, visible};
use crate::document::{Atom, Document};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct HiddenLabelNotes {
    pub hydrogens: Option<String>,
    pub charges: Option<String>,
}

/// Explain suppressed label ink, using the same visibility and H counts as drawing.
pub fn hidden_label_notes(doc: &Document, atoms: &[&Atom]) -> HiddenLabelNotes {
    let mut hydrogen_forms = Vec::new();
    let mut charge_forms = Vec::new();
    let mut hydrogen_count = 0;
    let mut charge_count = 0;
    for &atom in atoms {
        if !doc.atom_visible(atom.id)
            || crate::models3d::hides(doc, atom.id)
            || doc.abbreviation(atom.id).is_some()
            || doc.generic.owns_label(atom.id)
            || atom.element == "*"
            || !visible(atom, doc)
        {
            continue;
        }
        let h = if atom.no_implicit {
            atom.explicit_h
        } else {
            atom.label_h.max(atom.explicit_h)
        };
        if atom.element != "H" && h > 0 && !hydrogens(atom, doc) {
            hydrogen_count += 1;
            let count = if h == 1 { String::new() } else { h.to_string() };
            let form = format!("{}, not {}H{count}", atom.element, atom.element);
            if !hydrogen_forms.contains(&form) {
                hydrogen_forms.push(form);
            }
        }
        // A charge centred in an aromatic circle stays drawn.
        if atom.charge != 0
            && atom.display.hide_charge
            && !crate::aromatic::charge_is_centered(doc, atom)
        {
            charge_count += 1;
            let count = match atom.charge.unsigned_abs() {
                1 => String::new(),
                n => n.to_string(),
            };
            let sign = if atom.charge > 0 { '+' } else { '−' };
            let form = format!("{}{count}{sign}", atom.element);
            if !charge_forms.contains(&form) {
                charge_forms.push(form);
            }
        }
    }
    let note = |forms: Vec<String>, count, prefix| {
        (count > 0).then(|| {
            let mut listed = forms.iter().take(3).cloned().collect::<Vec<_>>().join("; ");
            if forms.len() > 3 {
                listed.push_str("; …");
            }
            let plural = if count == 1 { "" } else { "s" };
            format!("{prefix}{listed} ({count} atom{plural} affected)")
        })
    };
    HiddenLabelNotes {
        hydrogens: note(hydrogen_forms, hydrogen_count, "Readers will see "),
        charges: note(
            charge_forms,
            charge_count,
            "Readers will not see the charge on ",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{atom_labels::Carbons, document::Point};

    fn notes(doc: &Document) -> HiddenLabelNotes {
        hidden_label_notes(doc, &doc.atoms.iter().collect::<Vec<_>>())
    }

    #[test]
    fn hidden_label_hydrogens_explain_aminophenol_and_respect_scope() {
        let mut doc = Document::default();
        let ring: Vec<_> = (0..6)
            .map(|_| doc.add_atom("C", Point::default()))
            .collect();
        for i in 0..6 {
            doc.add_bond(ring[i], ring[(i + 1) % 6], 1 + (i % 2) as u8, "plain");
        }
        let o = doc.add_atom("O", Point::default());
        let n = doc.add_atom("N", Point::default());
        doc.add_bond(ring[0], o, 1, "plain");
        doc.add_bond(ring[3], n, 1, "plain");
        crate::atom_labels::refresh::Refresh::calculate(&doc, &Default::default())
            .unwrap()
            .apply(&mut doc);
        assert_eq!(notes(&doc), HiddenLabelNotes::default());
        doc.atom_labels.hydrogens = false;
        assert_eq!(
            notes(&doc).hydrogens.as_deref(),
            Some("Readers will see O, not OH; N, not NH2 (2 atoms affected)")
        );
        assert_eq!(
            hidden_label_notes(&doc, &[doc.atom(n).unwrap()])
                .hydrogens
                .as_deref(),
            Some("Readers will see N, not NH2 (1 atom affected)")
        );
        doc.atom_mut(n).unwrap().display.hydrogens = Some(true);
        assert_eq!(
            notes(&doc).hydrogens.as_deref(),
            Some("Readers will see O, not OH (1 atom affected)")
        );
    }

    #[test]
    fn hidden_label_notes_only_count_visible_element_labels() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::default());
        doc.add_bond(a, b, 1, "plain");
        for atom in &mut doc.atoms {
            atom.label_h = 3;
            atom.display.hydrogens = Some(false);
            atom.charge = 1;
            atom.display.hide_charge = true;
        }
        assert_eq!(notes(&doc), HiddenLabelNotes::default());
        doc.atom_labels.carbons = Carbons::Terminal;
        assert_eq!(
            notes(&doc).hydrogens.as_deref(),
            Some("Readers will see C, not CH3 (2 atoms affected)")
        );
        assert_eq!(
            notes(&doc).charges.as_deref(),
            Some("Readers will not see the charge on C+ (2 atoms affected)")
        );
        doc.contract(&[a, b], "Et", "").unwrap();
        assert_eq!(notes(&doc), HiddenLabelNotes::default());
    }

    #[test]
    fn hidden_label_charge_note_only_counts_suppressed_nonzero_charges() {
        let mut doc = Document::default();
        let n = doc.add_atom("N", Point::default());
        doc.atom_mut(n).unwrap().charge = 1;
        assert_eq!(notes(&doc), HiddenLabelNotes::default());
        doc.atom_mut(n).unwrap().display.hide_charge = true;
        assert_eq!(
            notes(&doc).charges.as_deref(),
            Some("Readers will not see the charge on N+ (1 atom affected)")
        );
        doc.atom_mut(n).unwrap().charge = 0;
        assert_eq!(notes(&doc), HiddenLabelNotes::default());
        doc.atom_mut(n).unwrap().charge = 1;
        let mut model = crate::models3d::Model3d::new(
            &doc,
            vec![crate::models3d::Placed {
                id: n,
                xyz: [0.; 3],
            }],
            vec![],
            Default::default(),
        );
        model.display = crate::models3d::Display::ThreeD;
        doc.models.push(model);
        assert_eq!(notes(&doc), HiddenLabelNotes::default());
    }

    #[test]
    fn hidden_label_charge_note_skips_a_charge_centred_in_its_ring() {
        let mut doc = Document::default();
        let ids = crate::editing::ring(&mut doc, Point::default(), 6, true, 5.);
        let n = doc.atom_mut(ids[0]).unwrap();
        n.element = "N".into();
        n.charge = 1;
        n.display.hide_charge = true;
        assert!(notes(&doc).charges.is_some());
        crate::aromatic::toggle_center_charge(&mut doc, &ids).unwrap();
        assert_eq!(notes(&doc).charges, None);
    }

    #[test]
    fn hidden_label_form_cap_does_not_limit_affected_atom_count() {
        let mut doc = Document::default();
        doc.atom_labels.hydrogens = false;
        for element in ["O", "N", "S", "P", "N"] {
            let id = doc.add_atom(element, Point::default());
            doc.atom_mut(id).unwrap().label_h = 1;
        }
        assert_eq!(
            notes(&doc).hydrogens.as_deref(),
            Some("Readers will see O, not OH; N, not NH; S, not SH; … (5 atoms affected)")
        );
    }
}
