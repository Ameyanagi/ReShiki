//! Drawing gestures that place new structure.
use crate::app::App;
use crate::canvas::Tool;
use reshiki::document::{Document, Point};
use reshiki::editing;
use reshiki::graphics::Graphic;

impl App {
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
            10.0 / self.tab.camera.zoom,
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
                };
                match drawing.place(
                    &mut self.tab.doc,
                    start,
                    end,
                    constrain,
                    10. / self.tab.camera.zoom,
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
            self.tool = Tool::Select;
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
            match state.prepared.place(
                anchor,
                direction,
                10. / self.tab.camera.zoom,
                state.anchor,
                state.mode,
            ) {
                Ok((document, selected)) => {
                    let coordination = state.mode == reshiki::templates::Connection::Coordinate;
                    let unchanged = document == before;
                    self.tab.doc = document;
                    self.tab.selected = selected;
                    self.tab.joining = None;
                    self.tool = Tool::Select;
                    self.changed(before);
                    self.error = false;
                    self.status = if coordination {
                        if unchanged {
                            "This donor is already coordinated to that metal"
                        } else {
                            "Donor → metal contact added in place · Undo removes it"
                        }
                    } else {
                        "Fragments joined · Undo restores their original positions"
                    }
                    .into();
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
            10.0 / self.tab.camera.zoom,
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
    pub(super) fn place_ring_preset(
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
        match drawing.place(&self.tab.doc, anchor, direction, 10. / self.tab.camera.zoom) {
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
    pub(super) fn place_ring(
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
            10.0 / self.tab.camera.zoom,
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
