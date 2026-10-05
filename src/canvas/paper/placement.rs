//! Previews that place new structure onto the draft, stroking the conflict when a placement fails.

use super::Draft;
use crate::canvas::hit::{bond_target_with, plane_endpoint};
use crate::canvas::snapping::ring_gesture;
use crate::canvas::{Gesture, MoleculeCanvas, State, Tool, delocalized_ring_size, layered, rgb};
use iced::widget::canvas::{Path, Stroke};
use iced::{Color, Point, Rectangle};
use reshiki::chains;
use std::borrow::Cow;

impl MoleculeCanvas<'_> {
    pub(super) fn preview_chain(
        &self,
        draft: &mut Draft<'_>,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        let Draft {
            preview,
            ring_selection,
            chain_badge,
            ..
        } = draft;
        if let (
            Some(Gesture::Chain {
                start,
                pressed,
                source,
                points,
                snaking,
                dragged,
            }),
            Some(p),
        ) = (&state.gesture, state.cursor)
        {
            let cursor = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (points, target) = self.chain_plan(
                (*start, *pressed),
                *source,
                points,
                (*snaking, *dragged),
                cursor,
                state.modifiers,
            );
            let endpoint = target
                .and_then(|id| self.doc.atom(id).map(|a| a.position))
                .or_else(|| points.last().copied())
                .unwrap_or(*start);
            let added = points
                .len()
                .saturating_sub(usize::from(source.is_some()))
                .saturating_sub(usize::from(target.is_some() && points.len() > 1));
            let cancelled = *dragged && points.len() < 2;
            let placement = if cancelled {
                Ok((self.doc.clone(), vec![]))
            } else {
                chains::place(self.doc, &points, *source, target, 10.0 / self.camera.zoom)
            };
            match placement {
                Ok((doc, ids)) => {
                    *preview = Cow::Owned(doc);
                    *ring_selection = Some(ids);
                    *chain_badge = Some((
                        endpoint,
                        if cancelled {
                            "Release to cancel".into()
                        } else {
                            format!("{added} new C · {} bonds", points.len().saturating_sub(1))
                        },
                        true,
                    ));
                }
                Err(_) => {
                    for pair in points.windows(2) {
                        let [a, b] = pair else { continue };
                        frame.stroke(
                            &Path::line(
                                self.camera.screen(*a, bounds),
                                self.camera.screen(*b, bounds),
                            ),
                            Stroke::default()
                                .with_width(1.5)
                                .with_color(rgb([182, 66, 61])),
                        );
                    }
                    *chain_badge = Some((endpoint, "Overlap · change direction".into(), false));
                }
            }
        }
    }

    pub(super) fn preview_ring(&self, draft: &mut Draft<'_>, state: &State, bounds: Rectangle) {
        let Draft {
            preview,
            ring_selection,
            rejection,
            ..
        } = draft;
        if let Some(p) = state.cursor.filter(|p| bounds.contains(*p))
            && self.tool == Tool::Ring
        {
            let radius = 10.0 / self.camera.zoom;
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let placement = |anchor, direction| {
                reshiki::editing::ring_placement(
                    preview,
                    anchor,
                    self.ring_size,
                    self.aromatic_ring
                        || delocalized_ring_size(self.tool, self.ring_size, state.modifiers)
                            .is_some(),
                    radius,
                    direction,
                )
            };
            match &state.gesture {
                Some(Gesture::Ring { start, attached }) => {
                    let (anchor, direction) = ring_gesture(*start, end, *attached, radius);
                    match placement(anchor, direction) {
                        Ok((doc, ids)) => {
                            *preview = Cow::Owned(doc);
                            *ring_selection = Some(ids);
                        }
                        Err(reason) => *rejection = Some(reason),
                    }
                }
                // Hovering where a click would attach shows only a rejection.
                None if self.doc.nearest(end, radius).is_some()
                    || reshiki::editing::nearest_bond(self.doc, end, radius).is_some() =>
                {
                    *rejection = placement(end, None).err();
                }
                _ => {}
            }
        }
    }

    pub(super) fn preview_ring_preset(
        &self,
        draft: &mut Draft<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        let Draft {
            preview,
            ring_selection,
            template_notice,
            rejection,
            ..
        } = draft;
        if let Tool::RingPreset(preset) = self.tool
            && let Some(p) = state.cursor.filter(|p| bounds.contains(*p))
            && delocalized_ring_size(self.tool, self.ring_size, state.modifiers).is_none()
        {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (anchor, direction) = if let Some(Gesture::Ring { start, .. }) = &state.gesture {
                ring_gesture(*start, end, true, 10. / self.camera.zoom)
            } else {
                (end, None)
            };
            let drawing = reshiki::rings::Drawing {
                preset,
                length: self.bond_drawing.length,
                alternate: state.modifiers.shift(),
                connect: state.modifiers.alt(),
            };
            match drawing.place(self.doc, anchor, direction, 10. / self.camera.zoom) {
                Ok((doc, ids)) => {
                    *preview = Cow::Owned(doc);
                    for b in &mut preview.to_mut().bonds {
                        if self.doc.atom(b.a).is_none() || self.doc.atom(b.b).is_none() {
                            b.color = reshiki::palette::Color::Palette(
                                reshiki::palette::Hue::Teal,
                                reshiki::palette::Row::Strong,
                            );
                        }
                    }
                    *ring_selection = Some(ids);
                    *template_notice = Some((
                        format!(
                            "{preset} · Drag to orient · Alt connects by a bond · Escape cancels"
                        ),
                        true,
                    ));
                }
                Err(error) => {
                    let radius = 10. / self.camera.zoom;
                    *template_notice = Some((error.into(), false));
                    *rejection = Some(reshiki::editing::RingRejection {
                        message: error,
                        label: reshiki::editing::attachment_label(self.doc, anchor, radius),
                        outline: vec![],
                        atom: self.doc.nearest(anchor, radius),
                    });
                }
            }
        }
    }

    pub(super) fn preview_template(
        &self,
        draft: &mut Draft<'_>,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        let Draft {
            preview,
            ring_selection,
            template_notice,
            ..
        } = draft;
        if self.tool == Tool::Template
            && let Some(p) = state.cursor.filter(|p| bounds.contains(*p))
        {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (anchor, direction) = if let Some(Gesture::Ring { start, .. }) = &state.gesture {
                self.template_gesture(*start, end, state.modifiers)
            } else {
                (end, None)
            };
            let placement = self
                .joining
                .map(|(joining, source_anchor)| {
                    joining.place(
                        anchor,
                        direction,
                        10. / self.camera.zoom,
                        source_anchor,
                        self.template_connection,
                    )
                })
                .or_else(|| {
                    self.template.map(|(template, source_anchor)| {
                        template
                            .place(
                                self.doc,
                                anchor,
                                direction,
                                10. / self.camera.zoom,
                                source_anchor,
                                self.template_connection,
                            )
                            .map_err(str::to_owned)
                    })
                });
            match placement {
                Some(Ok((document, ids))) => {
                    *template_notice = Some((
                        if self.joining.is_some() {
                            "Move & attach preview · Click or release to join · Escape cancels"
                                .into()
                        } else {
                            format!(
                                "Preview · {} new objects ({} atoms) · Shift/Ctrl drag snaps 15° · Escape cancels",
                                document
                                    .all_ids()
                                    .len()
                                    .saturating_sub(self.doc.all_ids().len()),
                                document.atoms.len().saturating_sub(self.doc.atoms.len())
                            )
                        },
                        true,
                    ));
                    *preview = Cow::Owned(document);
                    // Tint only transient objects. Commit calls the same pure
                    // placement operation again and retains saved/JACS colors.
                    let existing: std::collections::HashSet<_> = self
                        .joining
                        .map(|(j, _)| &j.base)
                        .unwrap_or(self.doc)
                        .all_ids()
                        .into_iter()
                        .collect();
                    use reshiki::palette::{Color as Paint, Hue, Row};
                    let tint = Paint::Palette(Hue::Teal, Row::Strong);
                    for atom in &mut preview.to_mut().atoms {
                        if !existing.contains(&atom.id) {
                            atom.text_style.get_or_insert_with(Default::default).color = tint;
                        }
                    }
                    for group in &mut preview.to_mut().abbreviations {
                        if !existing.contains(&group.anchor)
                            && let Some(style) = &mut group.label_style
                        {
                            style.color = tint;
                            group.label_color_override = true;
                        }
                    }
                    for bond in &mut preview.to_mut().bonds {
                        if !existing.contains(&bond.a) || !existing.contains(&bond.b) {
                            bond.color = tint;
                        }
                    }
                    for label in &mut preview.to_mut().annotations {
                        if !existing.contains(&label.id) {
                            label.format.style.color = tint;
                            for span in &mut label.format.spans {
                                span.style.color = tint;
                            }
                        }
                    }
                    for graphic in &mut preview.to_mut().graphics {
                        if !existing.contains(&graphic.id) {
                            graphic.style.stroke = tint;
                            if graphic.style.fill.is_some() {
                                graphic.style.fill = Some(Paint::Palette(Hue::Teal, Row::Tint));
                            }
                        }
                    }
                    *ring_selection = Some(ids);
                }
                Some(Err(error)) => {
                    *template_notice = Some((error.to_string(), false));
                    frame.stroke(
                        &Path::circle(self.camera.screen(anchor, bounds), 10.0),
                        Stroke::default()
                            .with_width(2.0)
                            .with_color(Color::from_rgb8(182, 66, 61)),
                    );
                }
                None => {}
            }
        }
    }

    pub(super) fn preview_delocalized_ring(
        &self,
        draft: &mut Draft<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        let Draft {
            preview,
            ring_selection,
            rejection,
            ..
        } = draft;
        if let Tool::RingPreset(_) = self.tool
            && let Some(size) = delocalized_ring_size(self.tool, self.ring_size, state.modifiers)
            && let Some(p) = state.cursor.filter(|p| bounds.contains(*p))
        {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (anchor, direction) = if let Some(Gesture::Ring { start, .. }) = &state.gesture {
                ring_gesture(*start, end, true, 10. / self.camera.zoom)
            } else {
                (end, None)
            };
            match reshiki::editing::ring_placement(
                preview,
                anchor,
                size,
                true,
                10. / self.camera.zoom,
                direction,
            ) {
                Ok((doc, ids)) => {
                    *preview = Cow::Owned(doc);
                    *ring_selection = Some(ids);
                }
                Err(reason) => *rejection = Some(reason),
            }
        }
    }

    pub(super) fn preview_bonded_atom(
        &self,
        draft: &mut Draft<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        let Draft {
            preview,
            ring_selection,
            ..
        } = draft;
        if let (
            Some(Gesture::Draw {
                start,
                id: Some(id),
            }),
            Some(cursor),
            Tool::Atom,
        ) = (&state.gesture, state.cursor, self.tool)
        {
            let end = self
                .camera
                .world(Point::new(cursor.x - bounds.x, cursor.y - bounds.y), bounds);
            if start.distance(end) >= 3. / self.camera.zoom
                && bounds.contains(cursor)
                && let Some(origin) = self.doc.atom(*id).map(|a| a.position)
            {
                let (end, target) = bond_target_with(
                    self.doc,
                    origin,
                    end,
                    Some(*id),
                    12. / self.camera.zoom,
                    self.bond_drawing.unconstrained(state.modifiers.alt()),
                );
                let drawing = self.bond_drawing.unconstrained(state.modifiers.alt());
                let result = if let Some(endpoint) = target
                    .is_none()
                    .then(|| {
                        plane_endpoint(
                            self.doc,
                            *id,
                            self.camera.world(
                                Point::new(cursor.x - bounds.x, cursor.y - bounds.y),
                                bounds,
                            ),
                            drawing,
                        )
                    })
                    .flatten()
                {
                    reshiki::projection::growth::place(
                        self.doc,
                        *id,
                        endpoint,
                        self.element,
                        reshiki::bonds::BondPreset::Single,
                    )
                } else {
                    reshiki::editing::add_bonded_atom(self.doc, *id, end, target, self.element)
                };
                if let Ok((drawing, added)) = result {
                    *preview = Cow::Owned(drawing);
                    *ring_selection = Some(vec![*id, added]);
                }
            }
        }
    }

    pub(super) fn preview_bond(
        &self,
        draft: &mut Draft<'_>,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        let Draft {
            preview,
            ring_selection,
            ..
        } = draft;
        if let (Some(Gesture::Draw { start, id }), Some(cursor), Some(preset)) =
            (&state.gesture, state.cursor, self.tool.bond_preset())
        {
            let end = self
                .camera
                .world(Point::new(cursor.x - bounds.x, cursor.y - bounds.y), bounds);
            if start.distance(end) >= 3.0 / self.camera.zoom {
                let origin = id
                    .and_then(|id| self.doc.atom(id).map(|a| a.position))
                    .unwrap_or(*start);
                let (end, target) = bond_target_with(
                    self.doc,
                    origin,
                    end,
                    *id,
                    12.0 / self.camera.zoom,
                    self.bond_drawing.unconstrained(state.modifiers.alt()),
                );
                if preset != reshiki::bonds::BondPreset::Dotted
                    || id
                        .zip(target)
                        .is_some_and(|(a, b)| reshiki::bonds::hydrogen_endpoints(self.doc, a, b))
                {
                    let plane = id.filter(|_| target.is_none()).and_then(|id| {
                        plane_endpoint(
                            self.doc,
                            id,
                            self.camera.world(
                                Point::new(cursor.x - bounds.x, cursor.y - bounds.y),
                                bounds,
                            ),
                            self.bond_drawing.unconstrained(state.modifiers.alt()),
                        )
                        .map(|endpoint| (id, endpoint))
                    });
                    if let Some((id, endpoint)) = plane {
                        if let Ok((drawing, added)) =
                            reshiki::projection::growth::place(self.doc, id, endpoint, "C", preset)
                        {
                            *preview = Cow::Owned(drawing);
                            *ring_selection = Some(vec![id, added]);
                        }
                    } else {
                        let a = id.unwrap_or_else(|| preview.to_mut().add_atom("C", origin));
                        let z = target.unwrap_or_else(|| preview.to_mut().add_atom("C", end));
                        preset.place(preview.to_mut(), a, z);
                        *ring_selection = Some(vec![a, z]);
                    }
                } else {
                    frame.stroke(
                        &Path::circle(self.camera.screen(end, bounds), 8.0),
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(rgb([182, 66, 61])),
                    );
                }
            }
        }
    }
}
