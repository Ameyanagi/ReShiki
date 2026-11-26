//! Placed MO diagram edits and the orbital rows a picture is waiting for.
use super::{App, Choice, Figure, OrbitalRef, Settings, Stored, View, levels, science};
use reshiki::document::{Document, Point};
use reshiki::graphics::{Graphic, GraphicStyle, MoDiagram, MoPicture};
use reshiki::scene;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub(crate) enum DiagramEdit {
    Below(u8),
    Above(u8),
    Unlink,
    RemovePictures,
    Remove,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PendingPicture {
    epoch: u64,
    diagram: u64,
    molecule: u64,
    pub(super) picture: MoPicture,
}

impl PendingPicture {
    pub(super) fn valid_for(&self, epoch: u64, doc: &Document) -> bool {
        self.epoch == epoch
            && diagram(doc, self.diagram).is_some_and(|mo| {
                mo.molecule == Some(self.molecule)
                    && mo.result == self.picture.figure.orbital.result
            })
    }
}

fn diagram(doc: &Document, id: u64) -> Option<&MoDiagram> {
    doc.graphics.iter().find(|g| g.id == id)?.mo.as_deref()
}

pub(super) fn has_picture(mo: &MoDiagram, column: u8, index: usize) -> bool {
    mo.pictures
        .iter()
        .any(|p| p.column == column && p.figure.orbital.index as usize == index)
}

/// Keep the graphic's origin and the scale and direction of both axes.
fn edit_layout(doc: &mut Document, id: u64, edit: impl FnOnce(&mut MoDiagram)) {
    if let Some(g) = doc.graphics.iter_mut().find(|g| g.id == id)
        && let Some(mo) = &mut g.mo
    {
        let (w, h) = mo.natural_size(&doc.drawing_style);
        edit(mo);
        let (new_w, new_h) = mo.natural_size(&doc.drawing_style);
        g.axis_x = Point::new(g.axis_x.x * (new_w / w), g.axis_x.y * (new_w / w));
        g.axis_y = Point::new(g.axis_y.x * (new_h / h), g.axis_y.y * (new_h / h));
    }
}

impl App {
    pub(super) fn xtb_mo_place(&mut self, for_selection: bool) {
        let target = if for_selection && !self.xtb_mo_show_selection_result() {
            None
        } else {
            self.xtb_orbital_target()
        };
        let Some(target) = target else {
            self.status = "Run a calculation with orbitals first".into();
            self.error = false;
            return;
        };
        if let Some(id) = target.diagram {
            self.set_selection(vec![id]);
            return;
        }
        let (Some((stored_index, stored)), Some(frontier), Some(molecule)) = (
            self.xtb_orbital_stored(),
            self.xtb_orbital_frontier(&target)
                .filter(|_| target.problem.is_none()),
            self.xtb_shown()
                .and_then(|s| s.atoms.iter().flatten().next().copied()),
        ) else {
            self.status = "The frontier orbitals are not ready".into();
            self.error = false;
            return;
        };
        let choice = self.xtb_orbital_choice(&target);
        let template = self.xtb_orbital_figure(&target, &choice);
        let label = target.columns[choice.column].label.clone();
        let mut mo = MoDiagram::new(molecule, target.key, target.columns);
        let Some(column) = mo.columns.iter().position(|c| c.label == label) else {
            return;
        };
        let mut doc = self.tab.doc.clone();
        for (index, _) in frontier {
            let mut figure = template.clone();
            figure.orbital.index = index as u32;
            let Some((basis, row)) = self.xtb_orbital_row(stored, figure.orbital.spin, index)
            else {
                return;
            };
            if let Err(error) = science::pin_orbital(
                &mut doc,
                stored_index,
                figure.orbital.spin,
                index as u32,
                basis,
                row,
            ) {
                self.status = format!("Could not keep the orbitals: {error}");
                self.error = true;
                return;
            }
            mo.pictures.push(MoPicture {
                column: column as u8,
                figure,
            });
        }
        let (width, height) = mo.natural_size(&doc.drawing_style);
        let size = Point::new(width, height);
        let Some(origin) = diagram_origin(&doc, molecule, size) else {
            return;
        };
        let id = doc.next_id();
        let style = GraphicStyle {
            width_pt: doc.drawing_style.line_width_pt,
            ..Default::default()
        };
        doc.graphics
            .push(Graphic::mo_diagram(id, origin, size, mo, style));
        self.xtb_follow_results(&doc);
        let before = std::mem::replace(&mut self.tab.doc, doc);
        self.error = false;
        self.changed(before);
        if !self.error {
            self.set_selection(vec![id]);
            self.status = "Placed the MO diagram · ⌘Z removes it".into();
        }
    }

    /// The command can open a calculation with the dock closed. An explicit
    /// selection takes precedence over a result the dock previously showed.
    fn xtb_mo_show_selection_result(&mut self) -> bool {
        let atoms = self.xtb_target();
        let shown = self
            .xtb_shown()
            .filter(|shown| shown.stored.result.orbitals.is_some());
        if shown.is_some_and(|shown| {
            self.tab.selected.is_empty()
                || atoms
                    .as_ref()
                    .is_some_and(|atoms| shown.atoms.iter().flatten().any(|id| atoms.contains(id)))
        }) {
            return true;
        }
        let doc = &self.tab.doc;
        let result = if let [id] = self.tab.selected.as_slice()
            && let Some(mo) = diagram(doc, *id)
        {
            self.xtb_mo_result(mo, None)
                .map(|(_, stored)| &stored.result)
        } else {
            atoms.and_then(|atoms| {
                doc.science.results.iter().rev().find_map(|stored| {
                    let attachment = doc.science.attachment(stored.attachment)?;
                    (stored.result.orbitals.is_some()
                        && attachment
                            .atoms
                            .iter()
                            .flatten()
                            .any(|id| atoms.contains(id)))
                    .then_some(&stored.result)
                })
            })
        };
        let Some(result) = result.cloned() else {
            return false;
        };
        self.xtb_show_result(&result);
        self.tab.xtb.palette.view = View::Orbitals;
        self.tab.xtb.palette.collapsed = false;
        true
    }

    pub(in crate::app) fn xtb_mo_level_click(&mut self, id: u64, column: u8, index: usize) {
        if self.tab.selected.as_slice() != [id] {
            return;
        }
        if let Some(mo) = diagram(&self.tab.doc, id)
            && let Some((_, stored)) = self.xtb_mo_result(mo, None)
            && let Some(spectrum) = mo.columns.get(usize::from(column))
            && let Some(orbitals) = &stored.result.orbitals
            && let Some(tab_column) = levels::columns(orbitals)
                .iter()
                .position(|c| c.label == spectrum.label)
        {
            let choice = Choice {
                result: mo.result.clone(),
                column: tab_column,
                index,
                from_diagram: true,
            };
            let result = Arc::clone(&stored.result);
            self.xtb_end_preview();
            self.xtb_show_result(&result);
            self.tab.xtb.palette.view = View::Orbitals;
            self.tab.xtb.orbitals.choice = Some(choice);
            self.tab.xtb.orbitals.pending = None;
        }
        self.xtb_mo_toggle_picture(id, column, index);
    }

    pub(super) fn xtb_mo_toggle_picture(&mut self, id: u64, column: u8, index: usize) {
        let Some(mo) = diagram(&self.tab.doc, id) else {
            return;
        };
        let Some(spectrum) = mo.columns.get(usize::from(column)) else {
            return;
        };
        if index >= spectrum.energies_ev.len() {
            return;
        }
        if has_picture(mo, column, index) {
            let before = self.tab.doc.clone();
            edit_layout(&mut self.tab.doc, id, |mo| {
                mo.pictures
                    .retain(|p| p.column != column || p.figure.orbital.index as usize != index);
            });
            self.error = false;
            self.changed(before);
            if !self.error {
                self.status = "Removed the orbital picture · ⌘Z restores it".into();
            }
            return;
        }
        if mo.pictures.len() >= 16 {
            self.status = "An MO diagram can hold up to 16 pictures".into();
            self.error = false;
            return;
        }
        let state = &self.tab.xtb.orbitals;
        let (iso, look) = self
            .xtb_orbital_target()
            .filter(|target| target.diagram == Some(id))
            .map_or((state.iso, state.look), |target| {
                self.xtb_orbital_settings(&target)
            });
        let picture = MoPicture {
            column,
            figure: Figure {
                orbital: OrbitalRef {
                    result: mo.result.clone(),
                    spin: spectrum.spin,
                    index: index as u32,
                },
                iso,
                look,
                snapshot: None,
            },
        };
        let request = mo.molecule.map(|molecule| PendingPicture {
            epoch: self.tab.file_epoch,
            diagram: id,
            molecule,
            picture,
        });
        let recomputable = self
            .xtb_mo_result(mo, None)
            .is_some_and(|(_, stored)| matches!(stored.result.settings, Settings::Xtb(_)));
        self.error = false;
        if let Some(request) = request {
            let pending = &mut self.tab.xtb.orbitals.pending_pictures;
            if let Some(at) = pending.iter().position(|p| {
                p.diagram == id
                    && p.picture.column == column
                    && p.picture.figure.orbital.index as usize == index
            }) {
                pending.remove(at);
                self.status = "Canceled adding the orbital picture".into();
                return;
            }
            if self.xtb_mo_add_picture(&request) {
                return;
            }
            if recomputable && super::super::available() {
                let state = &mut self.tab.xtb.orbitals;
                state
                    .errors
                    .retain(|(key, _)| *key != request.picture.figure.orbital.result);
                state.pending_pictures.push(request);
                self.status = "Computing the orbital… it is added when ready".into();
                return;
            }
        }
        self.status = "This orbital's shape is not saved with this result".into();
    }

    /// The diagram's calculation on this molecule, independent of the palette.
    fn xtb_mo_result(
        &self,
        mo: &MoDiagram,
        orbital: Option<&OrbitalRef>,
    ) -> Option<(usize, &Stored)> {
        let atom = mo.molecule?;
        let science = &self.tab.doc.science;
        let mut results = science
            .results
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, stored)| {
                science
                    .attachment(stored.attachment)
                    .is_some_and(|attachment| {
                        attachment.atoms.contains(&Some(atom))
                            && science::result_key(&stored.result) == mo.result
                    })
            });
        let newest = results.next()?;
        if let Some(orbital) = orbital {
            let keeps_row = |stored: &Stored| {
                stored.result.orbitals.as_ref().is_some_and(|orbitals| {
                    super::compute::row(orbitals, orbital.spin, orbital.index as usize).is_some()
                })
            };
            if !keeps_row(newest.1) {
                return Some(
                    results
                        .find(|(_, stored)| keeps_row(stored))
                        .unwrap_or(newest),
                );
            }
        }
        Some(newest)
    }

    fn xtb_mo_add_picture(&mut self, request: &PendingPicture) -> bool {
        let id = request.diagram;
        let Some(mo) = diagram(&self.tab.doc, id) else {
            return true;
        };
        let picture = &request.picture;
        let orbital = &picture.figure.orbital;
        let index = orbital.index as usize;
        if mo.pictures.len() >= 16 || has_picture(mo, picture.column, index) {
            return true;
        }
        let Some((stored_index, stored)) = self.xtb_mo_result(mo, Some(orbital)) else {
            return false;
        };
        let Some((basis, row)) = self.xtb_orbital_row(stored, orbital.spin, index) else {
            return false;
        };
        let mut doc = self.tab.doc.clone();
        if let Err(error) = science::pin_orbital(
            &mut doc,
            stored_index,
            orbital.spin,
            orbital.index,
            basis,
            row,
        ) {
            self.status = format!("Could not keep the orbital: {error}");
            self.error = true;
            return true;
        }
        edit_layout(&mut doc, id, |mo| mo.pictures.push(picture.clone()));
        self.xtb_follow_results(&doc);
        let before = std::mem::replace(&mut self.tab.doc, doc);
        self.error = false;
        self.changed(before);
        if !self.error {
            self.status = "Added the orbital picture · ⌘Z removes it".into();
        }
        true
    }

    pub(super) fn xtb_mo_recompute(
        &self,
    ) -> Option<(
        Arc<reshiki::qcdata::CalcResult>,
        Arc<reshiki::qcdata::Frame>,
        Vec<u32>,
    )> {
        let state = &self.tab.xtb.orbitals;
        if state.computing.is_some() || !super::super::available() {
            return None;
        }
        let request = state.pending_pictures.first()?;
        let mo = diagram(&self.tab.doc, request.diagram)?;
        let (_, stored) = self.xtb_mo_result(mo, None)?;
        let frame = self.tab.doc.science.frame(&stored.result.frame)?;
        let rows = state
            .pending_pictures
            .iter()
            .filter_map(|pending| {
                let orbital = &pending.picture.figure.orbital;
                (orbital.result == mo.result).then_some(orbital.index)
            })
            .collect();
        Some((Arc::clone(&stored.result), Arc::clone(frame), rows))
    }

    pub(super) fn xtb_mo_finish_pictures(&mut self, key: &str) {
        for request in std::mem::take(&mut self.tab.xtb.orbitals.pending_pictures) {
            if request.valid_for(self.tab.file_epoch, &self.tab.doc)
                && (request.picture.figure.orbital.result != key
                    || !self.xtb_mo_add_picture(&request))
            {
                self.tab.xtb.orbitals.pending_pictures.push(request);
            }
        }
    }

    pub(in crate::app) fn xtb_mo_cancel_pending(&mut self) {
        self.tab.xtb.orbitals.pending_pictures.clear();
    }

    pub(super) fn xtb_mo_edit(&mut self, id: u64, edit: DiagramEdit) {
        if diagram(&self.tab.doc, id).is_none() {
            return;
        }
        let before = self.tab.doc.clone();
        match edit {
            DiagramEdit::Remove => self.tab.doc.delete(&[id]),
            DiagramEdit::Below(value) => {
                edit_layout(&mut self.tab.doc, id, |mo| mo.below = value.min(8))
            }
            DiagramEdit::Above(value) => {
                edit_layout(&mut self.tab.doc, id, |mo| mo.above = value.min(8))
            }
            DiagramEdit::Unlink => edit_layout(&mut self.tab.doc, id, |mo| mo.linked = false),
            DiagramEdit::RemovePictures => {
                edit_layout(&mut self.tab.doc, id, |mo| mo.pictures.clear())
            }
        }
        if matches!(edit, DiagramEdit::Remove | DiagramEdit::RemovePictures) {
            self.tab
                .xtb
                .orbitals
                .pending_pictures
                .retain(|p| p.diagram != id);
        }
        self.error = false;
        self.changed(before);
        if !self.error && matches!(edit, DiagramEdit::Remove) {
            self.status = "Removed the MO diagram · ⌘Z restores it".into();
        }
    }
}

fn diagram_origin(doc: &Document, molecule: u64, size: Point) -> Option<Point> {
    let atoms = reshiki::molecule_names::component(doc, molecule);
    let (lo, hi) = scene::selection_bounds(doc, &atoms)?;
    let gap = doc.drawing_style.bond_length_world;
    let right = Point::new(hi.x + gap, lo.y);
    let others: Vec<_> = doc.object_ids().filter(|id| !atoms.contains(id)).collect();
    let groups = reshiki::editing::groups(doc, &others);
    let boxes = scene::selections_bounds(doc, &groups);
    let overlaps = boxes.iter().flatten().any(|(lo, hi)| {
        lo.x < right.x + size.x && right.x < hi.x && lo.y < right.y + size.y && right.y < hi.y
    });
    if overlaps {
        let bottom = boxes
            .iter()
            .flatten()
            .fold(hi.y, |bottom, (_, hi)| bottom.max(hi.y));
        Some(Point::new(lo.x, bottom + gap))
    } else {
        Some(right)
    }
}
