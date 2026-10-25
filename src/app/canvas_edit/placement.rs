//! Drawing gestures that place new structure.
use crate::app::{App, joining};
use crate::canvas::pick::PICK_RADIUS_PX;
use crate::canvas::{Edit, Tool};
use reshiki::document::{Document, Point};
use reshiki::editing;
use reshiki::graphics::Graphic;
use reshiki::joining::Placed;

impl App {
    pub(super) fn place_text_bond(&mut self, edit: Edit, before: Document) {
        let Edit::TextBond {
            source,
            source_text,
            start,
            end,
            target,
            target_text,
        } = edit
        else {
            return;
        };
        if !self.tool.bonds_from_text() {
            return;
        }
        let conversion = convert_bond_text(
            &self.tab.doc,
            (source, source_text),
            (target, target_text),
            (start, end),
        );
        let (doc, source, target) = match conversion {
            Ok(converted) => converted,
            Err(error) => {
                self.status = error;
                self.error = true;
                return;
            }
        };
        let revision = self.tab.revision;
        // A converted caption never stays in the drawing without its Undo step.
        let fallback = before.clone();
        self.tab.doc = doc;
        if let Some(end) = end {
            self.place_bond(start, end, source, target, before);
        } else if let Some(source) = source
            && self.default_bond_from_atom(source)
        {
            self.changed(before);
        }
        if self.tab.revision == revision {
            self.tab.doc = fallback;
            return;
        }
        self.status = "Bond from text".into();
        self.error = false;
    }

    pub(super) fn place_chain_from_text(
        &mut self,
        points: &[Point],
        source: Option<u64>,
        source_text: Option<u64>,
        target: Option<u64>,
        before: Document,
    ) {
        let Some(text) = source_text else {
            return self.place_chain(points, source, target, before);
        };
        if !self.tool.bonds_from_text() {
            return;
        }
        let result =
            reshiki::atom_text::text_to_atom_toward(&self.tab.doc, text, points.get(1).copied())
                .and_then(|(doc, source)| {
                    reshiki::chains::place(
                        &doc,
                        points,
                        Some(source),
                        target,
                        self.tab.camera.world_len(PICK_RADIUS_PX),
                    )
                });
        match result {
            Ok((doc, ids)) => {
                self.tab.doc = doc;
                self.tab.selected = ids;
            }
            Err(error) => {
                self.status = error;
                self.error = true;
                return;
            }
        }
        let revision = self.tab.revision;
        self.changed(before);
        if self.tab.revision == revision {
            return;
        }
        self.status = "Bond from text".into();
        self.error = false;
    }

    pub(super) fn place_chain(
        &mut self,
        points: &[Point],
        source: Option<u64>,
        target: Option<u64>,
        before: Document,
    ) {
        match reshiki::chains::place(
            &self.tab.doc,
            points,
            source,
            target,
            self.tab.camera.world_len(PICK_RADIUS_PX),
        ) {
            Ok((doc, ids)) => {
                self.tab.doc = doc;
                self.tab.selected = ids;
            }
            Err(error) => {
                self.status = error;
                self.error = true;
                return;
            }
        }
        self.changed(before);
    }
    pub(super) fn place_graphic(
        &mut self,
        start: Point,
        end: Point,
        constrain: bool,
        snap_orbitals: bool,
        before: Document,
    ) {
        if let Tool::Graphic(kind) = self.tool {
            if matches!(
                kind,
                reshiki::graphics::GraphicKind::Symbol(_)
                    | reshiki::graphics::GraphicKind::Orbital(_)
            ) {
                let drawing = reshiki::scientific::Drawing {
                    kind,
                    style: self.tab.graphic_style.clone(),
                    phase: self.tab.orbital_phase,
                    flipped: self.tab.phase_flipped,
                    attach: self.tab.attach_symbols,
                    snap_orbitals,
                };
                match drawing.place(
                    &mut self.tab.doc,
                    start,
                    end,
                    constrain,
                    self.tab.camera.world_len(PICK_RADIUS_PX),
                ) {
                    Ok(id) => self.tab.selected = vec![id],
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                        return;
                    }
                }
            } else {
                let id = self.tab.doc.next_id();
                self.tab.doc.graphics.push(
                    Graphic::dragged(
                        id,
                        kind,
                        start,
                        end,
                        self.tab.graphic_style.clone(),
                        self.tab.bracket_sides,
                        constrain,
                    )
                    .with_arc(self.tab.arc_editor.geometry),
                );
                self.tab.selected = vec![id];
            }
            if !matches!(kind, reshiki::graphics::GraphicKind::Symbol(_)) {
                self.tool = Tool::Select;
            }
        }
        self.changed(before);
    }
    pub(super) fn place_template(
        &mut self,
        anchor: Point,
        direction: Option<Point>,
        before: Document,
    ) {
        if let Some(state) = &self.tab.joining {
            if state.revision != self.tab.revision || state.epoch != self.tab.file_epoch {
                self.cancel_join();
                self.status = "The drawing changed. Start Move & attach again.".into();
                self.error = true;
                return;
            }
            match state.prepared.place_outcome(
                anchor,
                direction,
                self.tab.camera.world_len(PICK_RADIUS_PX),
                state.anchor,
                state.mode,
            ) {
                Ok((outcome, selected)) => {
                    let status = match &outcome {
                        Placed::Coordinated(coordination) => {
                            joining::coordination_status(coordination, false)
                        }
                        Placed::Joined(_) => {
                            "Fragments joined · Undo restores their original positions"
                        }
                    };
                    self.tab.doc = outcome.into_document(&state.prepared.original);
                    self.tab.selected = selected;
                    self.tab.joining = None;
                    self.tool = Tool::Select;
                    self.changed(before);
                    self.error = false;
                    self.status = status.into();
                    self.sync_typography();
                }
                Err(error) => {
                    self.status = error;
                    self.error = true;
                }
            }
            return;
        }
        if self.tool != Tool::Template {
            return;
        }
        let Some(template) = self.templates.library.get(self.template_index) else {
            return;
        };
        match template.place(
            &self.tab.doc,
            anchor,
            direction,
            self.tab.camera.world_len(PICK_RADIUS_PX),
            self.templates.anchor,
            self.templates.connection,
        ) {
            Ok((document, selected)) => {
                self.tab.doc = document;
                self.tab.selected = selected;
                if !self.templates.repeat {
                    self.tool = Tool::Select;
                }
            }
            Err(error) => {
                self.status = error.into();
                self.error = true;
                return;
            }
        }
        self.changed(before);
    }
    pub(in crate::app) fn place_ring_preset(
        &mut self,
        preset: reshiki::rings::Preset,
        anchor: Point,
        direction: Option<Point>,
        connect: bool,
        alternate: bool,
        before: Document,
    ) {
        let drawing = reshiki::rings::Drawing {
            preset,
            length: self.tab.bond_drawing.length,
            alternate,
            connect,
        };
        match drawing.place(
            &self.tab.doc,
            anchor,
            direction,
            self.tab.camera.world_len(PICK_RADIUS_PX),
        ) {
            Ok((doc, ids)) => {
                self.tab.doc = doc;
                self.tab.selected = ids;
            }
            Err(error) => {
                self.status = error.into();
                self.error = true;
                return;
            }
        }
        self.changed(before);
    }
    pub(in crate::app) fn place_ring(
        &mut self,
        anchor: Point,
        direction: Option<Point>,
        size: u8,
        aromatic: bool,
        before: Document,
    ) {
        match editing::ring_oriented(
            &mut self.tab.doc,
            anchor,
            size,
            aromatic,
            self.tab.camera.world_len(PICK_RADIUS_PX),
            direction,
        ) {
            Ok(ids) => self.tab.selected = ids,
            Err(error) => {
                self.status = error.into();
                self.error = true;
                return;
            }
        }
        self.changed(before);
    }
    pub(super) fn place_plane_bond(
        &mut self,
        start: u64,
        end: reshiki::projection::growth::Endpoint,
        before: Document,
    ) {
        let preset = self
            .tool
            .bond_preset()
            .unwrap_or(reshiki::bonds::BondPreset::Single);
        if preset == reshiki::bonds::BondPreset::Dotted {
            self.status = "Drag from a bonded explicit H to an existing acceptor".into();
            self.error = true;
            return;
        }
        let element = if self.tool == Tool::Atom {
            self.element.as_str()
        } else {
            "C"
        };
        match reshiki::projection::growth::place(&self.tab.doc, start, end, element, preset) {
            Ok((doc, id)) => {
                self.tab.doc = doc;
                self.tab.selected = vec![id];
            }
            Err(error) => {
                self.status = error;
                self.error = true;
                return;
            }
        }
        self.changed(before);
    }
    pub(super) fn place_bond(
        &mut self,
        start: Point,
        end: Point,
        a: Option<u64>,
        b: Option<u64>,
        before: Document,
    ) {
        if self.tool.bond_preset() == Some(reshiki::bonds::BondPreset::Dotted)
            && !a
                .zip(b)
                .is_some_and(|(a, b)| reshiki::bonds::hydrogen_endpoints(&self.tab.doc, a, b))
        {
            self.status =
                "Drag from a bonded explicit H to an existing N, O, F or S acceptor".into();
            self.error = true;
            return;
        }
        if self.tool == Tool::Atom {
            let result = a
                .ok_or_else(|| "Start the drag on an existing atom".to_string())
                .and_then(|id| editing::add_bonded_atom(&self.tab.doc, id, end, b, &self.element));
            match result {
                Ok((doc, id)) => {
                    self.tab.doc = doc;
                    self.tab.selected = vec![id];
                }
                Err(error) => {
                    self.status = error;
                    self.error = true;
                    return;
                }
            }
        } else if self.tool == Tool::Arrow {
            self.place_arrow(start, end);
        } else {
            let a = a.unwrap_or_else(|| self.tab.doc.add_atom("C", start));
            let b = b.unwrap_or_else(|| self.tab.doc.add_atom("C", end));
            if let Some(preset) = self.tool.bond_preset() {
                preset.place(&mut self.tab.doc, a, b);
            } else {
                let (order, display) = self.bond_style();
                self.tab.doc.add_bond(a, b, order, display);
            }
            self.tab.selected = vec![b];
        }
        self.changed(before);
    }
}

/// Convert both ends on a candidate so a refusal never installs half an edit.
/// Each end is read toward the other, as the bond will leave it.
fn convert_bond_text(
    doc: &Document,
    source: (Option<u64>, Option<u64>),
    target: (Option<u64>, Option<u64>),
    (start, end): (Point, Option<Point>),
) -> Result<(Document, Option<u64>, Option<u64>), String> {
    use reshiki::atom_text::text_to_atom_toward;
    let (doc, source) = if let Some(text) = source.1 {
        let (doc, id) = text_to_atom_toward(doc, text, end)?;
        (doc, Some(id))
    } else {
        (doc.clone(), source.0)
    };
    if let Some(text) = target.1 {
        let (doc, target) = text_to_atom_toward(&doc, text, Some(start))?;
        Ok((doc, source, Some(target)))
    } else {
        Ok((doc, source, target.0))
    }
}
