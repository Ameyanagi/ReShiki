//! Scheme layout measures rendered labels, coefficients and disconnected fragments.
use super::{DrawingSettings, Molecule, Proposal};
use crate::{
    document::{Annotation, Arrow, Document, Point},
    engine::{ChemistryEngine, PythonEngine, Request},
    typography::{TextAlign, TextFormat},
};
use std::collections::HashSet;

struct Participant {
    doc: Document,
    label: String,
    coefficient: u16,
    lo: Point,
    hi: Point,
    width: f32,
    label_format: TextFormat,
}
fn caption(doc: &mut Document, text: &str, center: Point, format: &TextFormat) {
    if text.is_empty() {
        return;
    }
    let size = crate::typography::layout(text, format);
    doc.annotations.push(Annotation {
        id: doc.next_id(),
        position: center.offset(-size.width / 2., 0.),
        text: text.into(),
        format: format.clone(),
    });
}
fn separate_components(doc: &mut Document, gap: f32) {
    let mut unseen: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    let mut components = Vec::new();
    for atom in &doc.atoms {
        if !unseen.remove(&atom.id) {
            continue;
        }
        let mut ids = vec![atom.id];
        let mut offset = 0;
        while let Some(id) = ids.get(offset).copied() {
            for bond in &doc.bonds {
                let other = if bond.a == id {
                    Some(bond.b)
                } else if bond.b == id {
                    Some(bond.a)
                } else {
                    None
                };
                if let Some(other) = other
                    && unseen.remove(&other)
                {
                    ids.push(other);
                }
            }
            offset += 1;
        }
        components.push(ids);
    }
    if components.len() < 2 {
        return;
    }
    let mut x = 0.;
    for ids in components {
        if let Some((lo, hi)) = crate::scene::selection_bounds(doc, &ids) {
            for atom in &mut doc.atoms {
                if ids.contains(&atom.id) {
                    atom.position = atom.position.offset(x - lo.x, -(lo.y + hi.y) / 2.);
                }
            }
            x += hi.x - lo.x + gap;
        }
    }
}
async fn prepare(
    engine: &PythonEngine,
    molecule: &Molecule,
    settings: &DrawingSettings,
) -> Result<Participant, String> {
    let parts: Vec<_> = molecule.smiles.split('.').collect();
    let repeated = parts.len() > 1
        && parts
            .first()
            .is_some_and(|first| parts.iter().all(|p| p == first));
    if repeated && molecule.coefficient > 1 {
        return Err("Use one copy of a molecule with a coefficient, not both repeated SMILES and a coefficient".into());
    }
    let smiles = if repeated {
        parts.first().copied().unwrap_or(&molecule.smiles)
    } else {
        &molecule.smiles
    };
    let coefficient = if repeated {
        u16::try_from(parts.len()).map_err(|_| "Too many repeated molecules")?
    } else {
        molecule.coefficient
    };
    if coefficient > 99 {
        return Err("Reaction coefficients must be at most 99".into());
    }
    let mut doc = engine
        .execute(Request::import_smiles(smiles))
        .await?
        .document
        .ok_or("The chemistry engine returned no molecule")?;
    if doc.atoms.len() > 300 {
        return Err("A proposed molecule exceeds 300 atoms".into());
    }
    let scale = settings.bond_length / crate::style::DEFAULT.bond_length_world;
    let angle = molecule.rotation.to_radians();
    let (sin, cos) = angle.sin_cos();
    let mut groups = Vec::new();
    for atom in &mut doc.atoms {
        let p = atom.position;
        atom.position = Point::new(
            (p.x * cos - p.y * sin) * scale,
            (p.x * sin + p.y * cos) * scale,
        );
        atom.text_style = Some(settings.format.style.clone());
        if atom.element == "*" && atom.map_num > 0 {
            groups.push((atom.id, format!("R{}", atom.map_num)));
        }
        if atom.element == "O"
            && atom.label_h == 2
            && !doc.bonds.iter().any(|b| b.a == atom.id || b.b == atom.id)
        {
            atom.display.hydrogen_position = crate::atom_labels::HydrogenPosition::Left;
        }
    }
    for (id, label) in groups {
        doc.contract(&[id], &label, &label)?;
    }
    for bond in &mut doc.bonds {
        bond.color = settings.bond_color;
    }
    doc.atom_labels = settings.labels.clone();
    separate_components(&mut doc, settings.bond_length * 0.8);
    let (lo, hi) =
        crate::scene::selection_bounds(&doc, &doc.all_ids()).ok_or("Empty proposed molecule")?;
    let mut label = molecule.label.clone();
    // A formula already represented by the structure/coefficient needs no duplicate caption.
    if smiles == "O"
        && ["H₂O", "H2O", "3 H₂O", "3 H2O", "Water", "water", "水"].contains(&label.as_str())
    {
        label.clear();
    }
    let mut label_format = settings.format.clone();
    label_format.alignment = TextAlign::Center;
    let max_label_width = (hi.x - lo.x).max(settings.bond_length * 4.5);
    let natural = crate::typography::layout(&label, &label_format).width;
    if natural > max_label_width {
        label_format.width_pt = Some(max_label_width * crate::style::DEFAULT.points_per_world());
    }
    let coefficient_width = if coefficient > 1 {
        crate::typography::layout(&coefficient.to_string(), &settings.format).width
            + settings.bond_length * 0.25
    } else {
        0.
    };
    let width = (hi.x - lo.x + coefficient_width)
        .max(crate::typography::layout(&label, &label_format).width);
    doc.validate()?;
    Ok(Participant {
        doc,
        label,
        coefficient,
        lo,
        hi,
        width,
        label_format,
    })
}
async fn prepare_all(
    engine: &PythonEngine,
    molecules: &[Molecule],
    settings: &DrawingSettings,
) -> Result<Vec<Participant>, String> {
    let mut result = Vec::new();
    for molecule in molecules {
        result.push(prepare(engine, molecule, settings).await?);
    }
    Ok(result)
}
fn place_row(
    doc: &mut Document,
    parts: &[Participant],
    x: &mut f32,
    y: f32,
    caption_y: f32,
    settings: &DrawingSettings,
    separators: bool,
) -> Result<f32, String> {
    let gap = settings.bond_length;
    let mut bottom = caption_y;
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            if separators {
                caption(
                    doc,
                    "+",
                    Point::new(*x + gap * 0.6, y - settings.format.style.size() * 0.5),
                    &settings.format,
                );
            }
            *x += gap * 1.2;
        }
        let coeff = if part.coefficient > 1 {
            part.coefficient.to_string()
        } else {
            String::new()
        };
        let coeff_width = if coeff.is_empty() {
            0.
        } else {
            crate::typography::layout(&coeff, &settings.format).width + gap * 0.25
        };
        let offset = (part.width - (part.hi.x - part.lo.x) - coeff_width) / 2.;
        caption(
            doc,
            &coeff,
            Point::new(
                *x + offset + (coeff_width - gap * 0.25) / 2.,
                y - settings.format.style.size() * 0.5,
            ),
            &settings.format,
        );
        let ids = crate::editing::append(
            doc,
            &part.doc,
            Point::new(
                *x + offset + coeff_width - part.lo.x,
                y - (part.lo.y + part.hi.y) / 2.,
            ),
        );
        if ids.is_empty() {
            return Err("Could not place proposed molecule".into());
        }
        caption(
            doc,
            &part.label,
            Point::new(*x + part.width / 2., caption_y),
            &part.label_format,
        );
        bottom = bottom
            .max(caption_y + crate::typography::layout(&part.label, &part.label_format).height);
        *x += part.width;
    }
    Ok(bottom)
}
pub async fn render(
    engine: &PythonEngine,
    proposal: &Proposal,
    settings: &DrawingSettings,
) -> Result<Document, String> {
    proposal.validate()?;
    let mut settings = settings.clone();
    settings.format.spans.clear();
    settings.format.width_pt = None;
    settings.format.validate("")?;
    if !settings.bond_length.is_finite() || !(4.0..=600.).contains(&settings.bond_length) {
        return Err("Invalid drawing bond length".into());
    }
    let gap = settings.bond_length;
    let mut doc = Document {
        atom_labels: settings.labels.clone(),
        ..Default::default()
    };
    let mut y = 0.;
    for chunk in proposal.molecules.chunks(3) {
        let parts = prepare_all(engine, chunk, &settings).await?;
        let height = parts.iter().map(|p| p.hi.y - p.lo.y).fold(gap, f32::max);
        y = place_row(
            &mut doc,
            &parts,
            &mut 0.,
            y,
            y + height / 2. + gap * 0.6,
            &settings,
            false,
        )? + gap * 2.;
    }
    for reaction in &proposal.reactions {
        let left = prepare_all(engine, &reaction.reactants, &settings).await?;
        let right = prepare_all(engine, &reaction.products, &settings).await?;
        let height = left
            .iter()
            .chain(&right)
            .map(|p| p.hi.y - p.lo.y)
            .fold(gap, f32::max);
        let caption_y = y + height / 2. + gap * 0.6;
        let mut x = 0.;
        let left_bottom = place_row(&mut doc, &left, &mut x, y, caption_y, &settings, true)?;
        x += gap;
        let mut conditions_format = settings.format.clone();
        conditions_format.alignment = TextAlign::Center;
        let natural = crate::typography::layout(&reaction.conditions, &conditions_format);
        if natural.width > gap * 4. {
            conditions_format.width_pt = Some(gap * 4. * crate::style::DEFAULT.points_per_world());
        }
        let conditions = crate::typography::layout(&reaction.conditions, &conditions_format);
        let width = conditions.width.max(gap * 2.8) + gap * 0.4;
        let mut style = settings.arrow_style.clone();
        let preset = crate::arrows::Preset::from_kind(&reaction.arrow);
        let preset_style = crate::arrows::ArrowStyle::preset(preset);
        style.head = preset_style.head;
        style.tail = preset_style.tail;
        style.shape = preset_style.shape;
        doc.arrows.push(Arrow::new(
            doc.next_id(),
            Point::new(x, y),
            Point::new(x + width, y),
            preset,
            style,
        ));
        caption(
            &mut doc,
            &reaction.conditions,
            Point::new(x + width / 2., y - conditions.height - gap * 0.45),
            &conditions_format,
        );
        x += width + gap;
        let right_bottom = place_row(&mut doc, &right, &mut x, y, caption_y, &settings, true)?;
        y = left_bottom.max(right_bottom) + gap * 3.;
    }
    doc.validate()?;
    Ok(doc)
}
