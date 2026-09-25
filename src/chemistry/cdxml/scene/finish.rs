use super::*;
use crate::atom_labels::{AtomDisplay, Number, StereoDisplay};

fn narrow(point: ImportPoint) -> Point {
    Point::new(point.x as f32, point.y as f32)
}
fn indicator(value: NativeStereo) -> Result<StereoDisplay> {
    Ok(StereoDisplay {
        show: Some(value.show),
        offset: value.offset.map(narrow),
        style: value
            .style
            .map(NativeTextStyle::into_document)
            .transpose()?
            .unwrap_or_else(crate::atom_labels::stereo_style),
    })
}
fn display(value: NativeAtomDisplay) -> Result<AtomDisplay> {
    Ok(AtomDisplay {
        hide_charge: false,
        variable: None,
        carbons: Some(value.carbons),
        hydrogens: Some(value.hydrogens),
        hydrogen_position: value.hydrogen_position,
        stereo: indicator(value.stereo)?,
        number: value
            .number
            .map(|n| -> Result<Number> {
                Ok(Number {
                    text: n.text,
                    offset: n.offset.map(narrow),
                    style: n
                        .style
                        .map(NativeTextStyle::into_document)
                        .transpose()?
                        .unwrap_or_else(crate::atom_labels::number_style),
                })
            })
            .transpose()?,
    })
}
fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}
impl CdxmlScene {
    /// Retain source chemistry and appearance without assigning bond orders,
    /// charges or CIP labels. Callers must report the validation warning and
    /// withhold molecular properties for this drawing.
    pub(crate) fn into_unchecked_drawing(self) -> Result<Document> {
        if self.conformer_3d.is_none() {
            return Err(SceneError::Invalid(
                "Unvalidated drawing requires source coordinates",
            ));
        }
        let mut document = Document::default();
        for (i, source) in self.molecule.state.graph.atoms.iter().enumerate() {
            let p = self.molecule.positions.get(i).ok_or(SceneError::Limit)?;
            let element = chemistry::ELEMENTS
                .get(usize::from(source.atomic_number))
                .ok_or(SceneError::Limit)?;
            let id = *self.molecule.ids.get(i).ok_or(SceneError::Limit)?;
            let map = self
                .molecule
                .state
                .metadata
                .atoms
                .get(i)
                .ok_or(SceneError::Limit)?
                .map_number;
            document.atoms.push(document::Atom {
                id,
                element: element.symbol.into(),
                position: Point::new((p.x * 28.0) as f32, (-p.y * 28.0) as f32),
                depth: 0.,
                centroid: Vec::new(),
                attachment: None,
                charge: i32::from(source.charge),
                isotope: u32::from(source.isotope),
                explicit_h: u32::from(source.explicit_hydrogens),
                no_implicit: source.no_implicit,
                aromatic: source.aromatic,
                radical_electrons: source.radical_electrons,
                label_h: u32::from(source.explicit_hydrogens)
                    + self
                        .molecule
                        .state
                        .valences
                        .get(i)
                        .ok_or(SceneError::Limit)?
                        .implicit_hydrogens,
                map_num: u32::try_from(map).map_err(|_| SceneError::Invalid("Invalid atom map"))?,
                display: Default::default(),
                cip_label: None,
                marks: Vec::new(),
                stereo: None,
                text_style: None,
            });
        }
        document.bonds = self
            .base
            .bonds
            .iter()
            .map(|b| b.bond.clone().into_document().map_err(SceneError::from))
            .collect::<Result<_>>()?;
        self.restore(&mut document)?;
        document
            .validate()
            .map_err(|_| SceneError::Invalid("Invalid preserved drawing"))?;
        Ok(document)
    }

    /// Apply the original full-CIP reconstruction, then the actual response
    /// boundary (Value f64 -> Document f32). The prepared conformer is retained
    /// at f64 throughout chemical reconstruction. Errors publish no drawing.
    pub fn into_document(self) -> Result<ImportedCdxml> {
        let planar = !self.conformer_3d.unwrap_or(false);
        let drawing = chemistry::document::for_import_scene(
            &self.molecule,
            self.conformer_3d.unwrap_or(false),
        )?;
        let labels = drawing.labels()?;
        let mut molecule = self.molecule.clone();
        let mut document = drawing.finish_with(labels, |document| self.restore(document))?;
        if planar && crate::haworth::interchange::restore(&mut document) {
            molecule = chemistry::document::prepare(&document)?;
        }
        Ok(ImportedCdxml { molecule, document })
    }
    fn restore(self, document: &mut Document) -> Result<()> {
        let generated: HashMap<_, _> = std::mem::take(&mut document.bonds)
            .into_iter()
            .map(|b| (pair(b.a, b.b), b))
            .collect();
        let chemical: HashMap<_, _> = self
            .molecule
            .state
            .graph
            .bonds
            .iter()
            .map(|b| {
                Ok((
                    pair(
                        *self.molecule.ids.get(b.a).ok_or(SceneError::Limit)?,
                        *self.molecule.ids.get(b.b).ok_or(SceneError::Limit)?,
                    ),
                    b.aromatic,
                ))
            })
            .collect::<Result<_>>()?;
        let circular: BTreeSet<_> = self
            .base
            .bonds
            .iter()
            .filter(|b| b.bond.order == 4)
            .flat_map(|b| [b.bond.a, b.bond.b])
            .collect();
        let mut bonds = Vec::with_capacity(self.base.bonds.len());
        for previous in self.base.bonds {
            let old = previous.bond.into_document()?;
            let key = pair(old.a, old.b);
            let mut bond = generated
                .get(&key)
                .cloned()
                .ok_or(SceneError::Invalid("Missing reconstructed CDXML bond"))?;
            if old.a != bond.a {
                bond.stereo_atoms.reverse();
            }
            bond.a = old.a;
            bond.b = old.b;
            if matches!(old.order, 1 | 2 | 4)
                && chemical.get(&key).copied().ok_or(SceneError::Limit)?
            {
                // Keep the source's alternating-bond placement. Chemical
                // kekulization may choose the opposite, equivalent assignment.
                bond.order = old.order;
            }
            bond.display = old.display;
            if bond.order == 4 && matches!(bond.display.as_str(), "bold" | "wedge") {
                bond.projection = true;
            }
            bond.secondary_display = old.secondary_display;
            bond.double_position = old.double_position;
            bond.z_order = old.z_order;
            bond.color = old.color;
            if let Some(value) = previous.indicator {
                bond.indicator = indicator(value)?;
            }
            bonds.push(bond);
        }
        if bonds.len() != generated.len() {
            return Err(SceneError::Invalid(
                "CDXML reconstruction lost molecular bonds",
            ));
        }
        document.bonds = bonds;
        let mut previous: HashMap<_, _> = self.base.atoms.into_iter().map(|a| (a.id, a)).collect();
        for (i, atom) in document.atoms.iter_mut().enumerate() {
            if circular.contains(&atom.id) {
                atom.explicit_h = u32::from(
                    self.molecule
                        .state
                        .graph
                        .atoms
                        .get(i)
                        .ok_or(SceneError::Limit)?
                        .explicit_hydrogens,
                );
            }
            if let Some(old) = previous.remove(&atom.id) {
                atom.text_style = old
                    .text_style
                    .map(NativeTextStyle::into_document)
                    .transpose()?;
                let hidden_charge = old
                    .marks
                    .iter()
                    .any(|m| m.hidden == Some(true) && m.kind.charge());
                atom.marks = old
                    .marks
                    .into_iter()
                    .filter(|m| m.hidden != Some(true))
                    .map(|m| crate::scientific::AtomMark {
                        kind: m.kind,
                        offset: narrow(m.offset),
                        angle: m.angle as f32,
                        size_pt: Some(m.size_pt as f32),
                    })
                    .collect();
                if let Some(value) = old.display {
                    atom.display = display(value)?;
                }
                atom.display.hide_charge = hidden_charge;
            }
        }
        if !previous.is_empty() {
            return Err(SceneError::Invalid("Missing reconstructed CDXML atom"));
        }
        document.drawing_style = self.base.drawing_style.into_document()?;
        document.atom_labels = self.base.atom_labels;
        document.annotations = self
            .base
            .annotations
            .into_iter()
            .map(|c| {
                let (text, format) = NativeText {
                    text: c.text,
                    format: c.format,
                }
                .into_document()?;
                Ok(document::Annotation {
                    id: c.id,
                    position: narrow(c.position),
                    text,
                    format,
                })
            })
            .collect::<Result<_>>()?;
        document.arrows = self
            .base
            .arrows
            .into_iter()
            .map(|a| a.into_document().map_err(SceneError::from))
            .collect::<Result<_>>()?;
        let mut budget = crate::pictures::exchange::Budget::default();
        document.graphics = self
            .base
            .graphics
            .into_iter()
            .map(|g| g.into_document(&mut budget).map_err(SceneError::from))
            .collect::<Result<_>>()?;
        document.groups = self.base.groups;
        document.ring_fills = self.base.ring_fills;
        document.abbreviations = self.base.abbreviations;
        for attachment in self.attachments {
            let atom = document
                .atom_mut(attachment.id)
                .ok_or(SceneError::Invalid("Missing attachment atom"))?;
            atom.element = "*".into();
            atom.no_implicit = true;
            atom.label_h = 0;
            atom.cip_label = None;
            atom.attachment = Some(attachment.kind);
            atom.centroid = attachment.members;
        }
        Ok(())
    }
}

/// Complete detached CDXML drawing import; analysis and runtime routing remain
/// separate. No Python service is called and no caller-owned state is changed.
pub fn import_cdxml(text: &str) -> Result<ImportedCdxml> {
    if text.trim().is_empty() {
        return Err(SceneError::Invalid("Enter a structure first"));
    }
    assemble_cdxml(&super::super::prepare_cdxml(text)?)?.into_document()
}
