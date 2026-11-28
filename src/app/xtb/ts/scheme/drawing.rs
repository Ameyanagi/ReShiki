//! Detached, editable copies of the minima and confirmed connections in a search.
use super::run::{DiscoveredPath, Report};
use crate::app::{App, Request, frames_import};
use reshiki::{
    arrow_labels::{ArrowLabel, Side},
    document::{Annotation, Arrow, Document, Point},
    models3d::{self, Display},
    qcdata::Frame,
    reactions::{Participant, Reaction},
};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

pub(super) struct Placed {
    pub document: Document,
    pub ids: Vec<u64>,
    pub warnings: Vec<String>,
}

pub(super) fn available(report: &Report) -> bool {
    report.steps.iter().flatten().any(|step| {
        step.discovered
            .as_ref()
            .is_some_and(|path| path.order.len() >= 2)
    })
}

fn empty_like(doc: &Document) -> Document {
    Document {
        drawing_style: doc.drawing_style.clone(),
        atom_labels: doc.atom_labels.clone(),
        canvas_theme: doc.canvas_theme,
        color_theme: doc.color_theme,
        custom_theme: doc.custom_theme.clone(),
        ..Default::default()
    }
}

/// The calculation-output importer owns bond perception and geometric stereo.
/// Do not copy the reactant's topology onto an intermediate after bond changes.
fn minimum(host: &Document, frame: &Frame) -> Result<Document, String> {
    frame.validate().map_err(|error| error.to_string())?;
    let (doc, _) = frames_import::drawing(frame)?;
    finish_minimum(host, frame, doc)
}

fn physical_atoms(doc: &Document, frame: &Frame) -> Result<Vec<u64>, String> {
    let [model] = doc.models.as_slice() else {
        return Err("The geometry needs one complete molecular model".into());
    };
    let sources = model.frame_sources();
    if sources.len() != frame.numbers.len() {
        return Err("The geometry cannot be drawn without omitting atoms".into());
    }
    sources
        .into_iter()
        .zip(&frame.numbers)
        .map(|(source, &number)| {
            let id = source.ok_or("The imported geometry has an unmapped physical atom")?;
            let atom = doc
                .atom(id)
                .ok_or("The imported geometry has an unavailable physical atom")?;
            if reshiki::editing::ELEMENTS
                .get(usize::from(number) - 1)
                .copied()
                != Some(atom.element.as_str())
            {
                return Err("The imported geometry changed physical atom order".into());
            }
            Ok(id)
        })
        .collect()
}

fn finish_minimum(host: &Document, frame: &Frame, mut doc: Document) -> Result<Document, String> {
    // Haptic anchors and other drawing-only nodes are not calculation atoms.
    let physical = physical_atoms(&doc, frame)?;
    doc.drawing_style = host.drawing_style.clone();
    doc.atom_labels = host.atom_labels.clone();
    doc.canvas_theme = host.canvas_theme;
    doc.color_theme = host.color_theme;
    doc.custom_theme = host.custom_theme.clone();
    // Each path already uses one common atom order, including transferred H.
    // Store that correspondence before the importer folds ordinary hydrogens.
    for (index, id) in physical.iter().enumerate() {
        if let Some(atom) = doc.atom_mut(*id) {
            atom.map_num = (index + 1) as u32;
        }
    }
    if let [id] = physical.as_slice() {
        let atom = doc
            .atom_mut(*id)
            .ok_or("The physical atom is unavailable")?;
        atom.charge = frame.charge;
        atom.radical_electrons = u8::try_from(frame.multiplicity - 1)
            .ok()
            .filter(|radicals| *radicals <= 4)
            .ok_or("The atom's spin cannot be represented in a drawing")?;
    } else {
        let restored = restore_electronic_state(host, &mut doc, frame, &physical);
        if (frame.charge != 0 || frame.multiplicity != 1) && !restored {
            return Err(
                "The geometry does not assign its total charge or spin to individual atoms".into(),
            );
        }
    }
    frames_import::fold_calculation_hydrogens(&mut doc, frame);
    models3d::set_display(&mut doc, 0, Display::TwoD);
    let mut request = Request::molecule("clean", doc);
    request.cleanup = Some(reshiki::cleanup::Options {
        keep_orientation: false,
        style_bond_length: true,
        ..Default::default()
    });
    let cleaned = reshiki::engine::native_cleanup::prepare(&request)
        .map_err(|error| format!("The calculated topology cannot be laid out: {error}"))?;
    // Cleanup verifies chemistry and stereo, while the hidden model retains
    // the calculated geometry for switching this editable copy back to 3D.
    Ok(cleaned.document)
}

/// Reuse authored formal charges and radicals only on an unchanged, complete
/// mapped graph. Model hydrogen parents supply the otherwise undrawn H bonds.
/// A new intermediate's changed topology must never inherit these assignments.
fn restore_electronic_state(
    host: &Document,
    drawing: &mut Document,
    frame: &Frame,
    physical: &[u64],
) -> bool {
    if frame.drawing_atoms.len() != physical.len() {
        return false;
    }
    let source: HashSet<_> = frame.drawing_atoms.iter().flatten().copied().collect();
    if source.is_empty()
        || host
            .bonds
            .iter()
            .any(|bond| source.contains(&bond.a) != source.contains(&bond.b))
    {
        return false;
    }
    let mut charge = 0_i64;
    let mut multiplicity = 1;
    for (id, original) in physical.iter().zip(&frame.drawing_atoms) {
        let Some(atom) = drawing.atom(*id) else {
            return false;
        };
        match original {
            Some(id) => {
                let Some(original) = host
                    .atom(*id)
                    .filter(|original| original.element == atom.element)
                else {
                    return false;
                };
                charge += i64::from(original.charge);
                if !original.singlet {
                    multiplicity += u32::from(original.radical_electrons);
                }
            }
            None if atom.element == "H" => {}
            None => return false,
        }
    }
    if charge != i64::from(frame.charge) || multiplicity != frame.multiplicity {
        return false;
    }
    let Ok(known) = reshiki::science::frame_bonds(host, &frame.drawing_atoms) else {
        return false;
    };
    let indices: HashMap<_, _> = physical
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    let mut perceived: Vec<_> = drawing
        .bonds
        .iter()
        .filter_map(|bond| {
            let (&a, &b) = (indices.get(&bond.a)?, indices.get(&bond.b)?);
            Some((a.min(b), a.max(b), bond.order))
        })
        .collect();
    perceived.sort_unstable();
    if known != perceived {
        return false;
    }
    for (id, original) in physical.iter().zip(&frame.drawing_atoms) {
        if let Some(original) = original.and_then(|id| host.atom(id))
            && let Some(atom) = drawing.atom_mut(*id)
        {
            atom.charge = original.charge;
            atom.radical_electrons = original.radical_electrons;
            atom.singlet = original.singlet;
        }
    }
    true
}

fn caption(doc: &mut Document, text: String, center: Point) -> u64 {
    let mut annotation = Annotation {
        id: doc.next_id(),
        position: center,
        text,
        format: reshiki::typography::TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
        angle: 0.,
    };
    annotation.position.x -= annotation.size().0 / 2.;
    let id = annotation.id;
    doc.annotations.push(annotation);
    id
}

struct Item {
    atoms: Vec<u64>,
    left: f32,
    right: f32,
}

fn row(
    host: &Document,
    path: &DiscoveredPath,
    step: usize,
    warnings: &mut Vec<String>,
) -> Result<Document, String> {
    let mut row = empty_like(host);
    let bond = host.drawing_style.bond_length_world;
    let mut x = 0.;
    let mut items = Vec::new();
    for (position, &index) in path.order.iter().enumerate() {
        let value = path
            .minima
            .get(index)
            .ok_or("The discovered path references a missing minimum")?;
        let label = if position == 0 {
            format!("Step {} · Reactant", step + 1)
        } else if position + 1 == path.order.len() {
            format!("Step {} · Product", step + 1)
        } else {
            format!("I{}.{}", step + 1, position)
        };
        let (atoms, right) = match minimum(host, &value.frame) {
            Ok(part) => {
                let (lo, hi) = reshiki::scene::selection_bounds(&part, &part.all_ids())
                    .ok_or("The calculated structure has no drawable bounds")?;
                let ids = reshiki::editing::append_preserving_layout(
                    &mut row,
                    &part,
                    Point::new(x - lo.x, -(lo.y + hi.y) / 2.),
                );
                if ids.is_empty() {
                    return Err("The calculated structure could not be inserted".into());
                }
                let atoms = ids
                    .into_iter()
                    .filter(|id| row.atom(*id).is_some())
                    .collect();
                let width = (hi.x - lo.x).max(bond);
                caption(
                    &mut row,
                    label,
                    Point::new(x + width / 2., (hi.y - lo.y) / 2. + bond * 0.45),
                );
                (atoms, x + width)
            }
            Err(reason) => {
                warnings.push(format!("{label}: {reason}"));
                let width = bond * 5.;
                caption(
                    &mut row,
                    format!("{label}\n2D structure unavailable"),
                    Point::new(x + width / 2., 0.),
                );
                (Vec::new(), x + width)
            }
        };
        items.push(Item {
            atoms,
            left: x,
            right,
        });
        x = right + bond * 5.;
    }
    for (position, ((from, to), (&a, &b))) in items
        .iter()
        .zip(items.iter().skip(1))
        .zip(path.order.iter().zip(path.order.iter().skip(1)))
        .enumerate()
    {
        let matches = |reactant, product| reactant == a && product == b;
        let gap = path
            .gaps
            .iter()
            .find(|gap| matches(gap.reactant, gap.product));
        let segment = path
            .segments
            .iter()
            .find(|segment| matches(segment.reactant, segment.product));
        let center = Point::new((from.right + to.left) / 2., 0.);
        if gap.is_some() || segment.is_none() || from.atoms.is_empty() || to.atoms.is_empty() {
            caption(
                &mut row,
                "Unresolved\nconnection".into(),
                center.offset(0., -bond * 0.5),
            );
            continue;
        }
        let arrow = row.next_id();
        row.arrows.push(Arrow::new(
            arrow,
            Point::new(from.right + bond * 0.5, 0.),
            Point::new(to.left - bond * 0.5, 0.),
            reshiki::arrows::Preset::Forward,
            Default::default(),
        ));
        let label = caption(
            &mut row,
            format!("TS{}.{}‡", step + 1, position + 1),
            center,
        );
        row.arrow_labels.push(ArrowLabel {
            arrow,
            annotation: label,
            side: Side::Above,
            gap: None,
        });
        let participants = |atoms: &[u64]| {
            reshiki::reactions::molecules(&row, atoms)
                .into_iter()
                .map(|atoms| Participant {
                    atoms,
                    coefficient: 1,
                })
                .collect()
        };
        let mut reaction = Reaction::new(arrow);
        reaction.reactants = participants(&from.atoms);
        reaction.products = participants(&to.atoms);
        reaction.annotations.push(label);
        row.reactions.push(reaction);
    }
    reshiki::arrow_labels::reconcile(&mut row, &Document::default());
    row.validate()?;
    Ok(row)
}

/// Build the whole insertion first. The caller publishes it with one ordinary
/// edit; no source molecule, arrow, caption, profile or layout is replaced.
pub(super) fn build(doc: &Document, report: &Report) -> Result<Placed, String> {
    doc.validate()?;
    let mut drawing = empty_like(doc);
    let mut warnings = Vec::new();
    let mut y = 0.;
    for (index, value) in report.steps.iter().enumerate() {
        let Some(path) = value
            .as_ref()
            .and_then(|step| step.discovered.as_ref())
            .filter(|path| path.order.len() >= 2)
        else {
            continue;
        };
        let part = row(doc, path, index, &mut warnings)?;
        let (lo, hi) = reshiki::scene::selection_bounds(&part, &part.all_ids())
            .ok_or("The discovered path has no drawable structures")?;
        if reshiki::editing::append_preserving_layout(
            &mut drawing,
            &part,
            Point::new(-lo.x, y - lo.y),
        )
        .is_empty()
        {
            return Err("The expanded scheme could not be inserted".into());
        }
        y += hi.y - lo.y + doc.drawing_style.bond_length_world * 2.;
    }
    if drawing.atoms.is_empty() {
        return Err(warnings
            .first()
            .cloned()
            .unwrap_or_else(|| "No discovered minima are ready to place".into()));
    }
    let (lo, hi) = reshiki::scene::selection_bounds(&drawing, &drawing.all_ids())
        .ok_or("The discovered path has no drawable structures")?;
    let size = Point::new(hi.x - lo.x, hi.y - lo.y);
    let origin = if doc.object_ids().next().is_some() {
        super::super::profile::profile_origin_for_ids(doc, &doc.all_ids(), size)
            .ok_or("There is no room to place the expanded scheme")?
    } else {
        Point::default()
    };
    let mut document = doc.clone();
    let ids = reshiki::editing::append_preserving_layout(
        &mut document,
        &drawing,
        Point::new(origin.x - lo.x, origin.y - lo.y),
    );
    if ids.is_empty() {
        return Err("The expanded scheme could not be inserted".into());
    }
    Ok(Placed {
        document,
        ids,
        warnings,
    })
}

impl App {
    pub(in crate::app) fn xtb_place_discovered_scheme(&mut self) {
        if self.tab.busy || self.tab.drawing_edit_active() || !self.xtb_scheme_current() {
            return;
        }
        let Some(state) = &self.tab.xtb.scheme else {
            return;
        };
        if state.busy() {
            return;
        }
        let placed = match build(&self.tab.doc, &state.work.report) {
            Ok(placed) => placed,
            Err(reason) => {
                self.status = reason;
                self.error = true;
                return;
            }
        };
        self.xtb_end_preview();
        let before = std::mem::replace(&mut self.tab.doc, placed.document);
        self.error = false;
        self.changed(before);
        if !self.error {
            self.tool = crate::canvas::Tool::Select;
            self.set_selection(placed.ids);
            if let Some((lo, hi)) =
                reshiki::scene::selection_bounds(&self.tab.doc, &self.tab.selected)
            {
                self.pan_to_show(lo, hi);
            }
            self.status = "Placed editable scheme · Inferred bonds · ⌘Z to undo".into();
            if !placed.warnings.is_empty() {
                self.status
                    .push_str(&format!(" · {}", placed.warnings.join(" · ")));
            }
        }
    }
}
