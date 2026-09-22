//! Scheme layout measures rendered labels, coefficients and disconnected fragments.
use super::{DrawingSettings, Molecule, Proposal};
use crate::{
    document::{Annotation, Arrow, Document, Point},
    engine::{ChemistryEngine, LocalEngine, Request},
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
    engine: &LocalEngine,
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
    doc.drawing_style = settings.drawing_style.clone();
    let ids = doc.all_ids();
    super::composition::straighten(&mut doc, &ids);
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
    if molecule.compact {
        let ids = doc.all_ids();
        super::composition::compact_chains(&mut doc, &ids)?;
    }
    separate_components(&mut doc, settings.bond_length * 0.8);
    let (lo, hi) =
        crate::scene::selection_bounds(&doc, &doc.all_ids()).ok_or("Empty proposed molecule")?;
    let mut label = molecule.label.clone();
    // Suppress only a formula already shown by the structure/coefficient.
    // Explicit names such as Water or 水 remain editable captions.
    if smiles == "O" {
        let formula = label
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .replace('₂', "2");
        if formula == "H2O" || formula == format!("{coefficient}H2O") {
            label.clear();
        }
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
    engine: &LocalEngine,
    molecules: &[Molecule],
    settings: &DrawingSettings,
    progress: Option<&tokio::sync::mpsc::Sender<super::codex::Progress>>,
    prepared: &mut usize,
    total: usize,
) -> Result<Vec<Participant>, String> {
    let mut result = Vec::new();
    for molecule in molecules {
        result.push(prepare(engine, molecule, settings).await?);
        *prepared += 1;
        if let Some(progress) = progress {
            let _ = progress.try_send(super::codex::Progress::Structures {
                completed: *prepared,
                total,
            });
        }
    }
    Ok(result)
}
fn place_row(
    doc: &mut Document,
    parts: &[Participant],
    x: &mut f32,
    y: f32,
    settings: &DrawingSettings,
    separators: bool,
) -> Result<(f32, Vec<crate::reactions::Participant>), String> {
    let mut participants = Vec::new();
    let gap = settings.bond_length;
    let mut bottom = y;
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
        participants.push(crate::reactions::Participant {
            atoms: ids
                .into_iter()
                .filter(|id| doc.atom(*id).is_some())
                .collect(),
            coefficient: part.coefficient,
        });
        let caption_y = y + (part.hi.y - part.lo.y) / 2. + gap * 0.6;
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
    Ok((bottom, participants))
}
pub async fn render(
    engine: &LocalEngine,
    proposal: &Proposal,
    settings: &DrawingSettings,
) -> Result<Document, String> {
    render_progress(engine, proposal, settings, None).await
}

pub async fn render_progress(
    engine: &LocalEngine,
    proposal: &Proposal,
    settings: &DrawingSettings,
    progress: Option<&tokio::sync::mpsc::Sender<super::codex::Progress>>,
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
        drawing_style: settings.drawing_style.clone(),
        atom_labels: settings.labels.clone(),
        ..Default::default()
    };
    let mut y = 0.;
    let mut panels = Vec::new();
    let mut prepared = 0;
    let total = proposal.molecules.len()
        + proposal
            .reactions
            .iter()
            .map(|r| r.reactants.len() + r.products.len())
            .sum::<usize>();
    for chunk in proposal.molecules.chunks(3) {
        let first_id = doc.next_id();
        let parts = prepare_all(engine, chunk, &settings, progress, &mut prepared, total).await?;
        y = place_row(&mut doc, &parts, &mut 0., y, &settings, false)?.0 + gap * 2.;
        panels.push((
            doc.all_ids()
                .into_iter()
                .filter(|id| *id >= first_id)
                .collect(),
            super::composition::Role::Reaction,
        ));
        if let Some(progress) = progress {
            let mut preview = doc.clone();
            compose(&mut preview, &panels, proposal)?;
            let _ = progress.try_send(super::codex::Progress::Preview(Box::new(preview)));
        }
    }
    for (index, reaction) in proposal.reactions.iter().enumerate() {
        let first_id = doc.next_id();
        let left = prepare_all(
            engine,
            &reaction.reactants,
            &settings,
            progress,
            &mut prepared,
            total,
        )
        .await?;
        let right = prepare_all(
            engine,
            &reaction.products,
            &settings,
            progress,
            &mut prepared,
            total,
        )
        .await?;
        let mut x = 0.;
        let (left_bottom, reactants) = place_row(&mut doc, &left, &mut x, y, &settings, true)?;
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
        let arrow_id = doc.next_id();
        doc.arrows.push(Arrow::new(
            arrow_id,
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
        let (right_bottom, products) = place_row(&mut doc, &right, &mut x, y, &settings, true)?;
        let panel_ids: Vec<_> = doc
            .all_ids()
            .into_iter()
            .filter(|id| *id >= first_id)
            .collect();
        if let Some((lo, hi)) = crate::scene::selection_bounds(&doc, &panel_ids) {
            let title = if reaction.title.is_empty()
                && proposal.composition.arrangement == super::composition::Arrangement::Grid
            {
                format!("Reaction {}", index + 1)
            } else {
                reaction.title.clone()
            };
            caption(
                &mut doc,
                &title,
                Point::new((lo.x + hi.x) / 2., lo.y - gap * 1.2),
                &settings.format,
            );
        }
        panels.push((
            doc.all_ids()
                .into_iter()
                .filter(|id| *id >= first_id)
                .collect(),
            reaction.role,
        ));
        doc.reactions.push(crate::reactions::Reaction {
            arrow: arrow_id,
            reactants,
            products,
            agents: vec![],
            annotations: doc
                .annotations
                .iter()
                .filter(|a| a.id >= first_id)
                .map(|a| a.id)
                .collect(),
        });
        y = left_bottom.max(right_bottom) + gap * 3.;
        if let Some(progress) = progress {
            let mut preview = doc.clone();
            compose(&mut preview, &panels, proposal)?;
            let _ = progress.try_send(super::codex::Progress::Preview(Box::new(preview)));
        }
    }
    compose(&mut doc, &panels, proposal)?;
    if doc.atoms.len() > 1500 {
        return Err("The scheme exceeds 1500 atoms; request fewer examples".into());
    }
    doc.validate()?;
    Ok(doc)
}

fn compose(
    doc: &mut Document,
    panels: &[(Vec<u64>, super::composition::Role)],
    proposal: &Proposal,
) -> Result<(), String> {
    if proposal.composition.arrangement == super::composition::Arrangement::Branching {
        super::branching::merge(doc, &proposal.reactions)?;
    } else {
        super::composition::arrange(doc, panels, &proposal.composition);
    }
    Ok(())
}
